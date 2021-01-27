mod utils;
mod zip_util;

use std::io;
// use std::io::*; //woe betide
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};
use shapefile::dbase::*;

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
extern {
    fn alert(s: &str);
}

#[wasm_bindgen(start)]
pub fn main() -> Result<(), JsValue> {
    utils::set_panic_hook();
    Ok(())
}

#[wasm_bindgen]
pub fn greet(name: &str) {
    alert(&format!("Hello {}, from shp-contour-wasm!", name));
}



#[wasm_bindgen]
pub async fn fetch_shp(url: String) -> Result<MarshallGeometry, JsValue> {
    let mut opts = RequestInit::new();
    opts.method("GET");
    opts.mode(RequestMode::Cors); //probably shouldn't need CORS actually

    let request = Request::new_with_str_and_init(&url, &opts)?;
    request.headers().set("Accept", "application/zip")?;

    let window = web_sys::window().unwrap();
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    assert!(resp_value.is_instance_of::<Response>());
    let resp: Response = resp_value.dyn_into().unwrap();

    let data = JsFuture::from(resp.array_buffer()?).await?;
    //get data into a form readable by other rust methods
    let d = js_sys::Uint8Array::new(&data);
    // if we used &[u8] rather than vec, it'd already implement Read trait 
    // (and may be more efficient, living on stack?)
    let v = io::Cursor::new(d.to_vec());
    let reader = io::BufReader::new(v);
    //compute results (should be similar to already implemented code)
    let (geo_3d, triangles) = shp_main(reader).expect("err");
    //marshall results back into JsValues (preferably SharedArrayBuffers)
    //perhaps a geometry struct that is also represented in TS
    
    unsafe {
        Ok(marshall_geometry_to_js(geo_3d, triangles))
    }
}

//wasm_bindgen types cannot have lifetime specifiers
//also seem to be pretty limited in available types, lots of 'copy is not specified' complaints on pub fields
//may need getters https://github.com/rustwasm/wasm-bindgen/issues/439
//for the time-being I may just return an array [geo_3d: Float32Array, triangles: Uint16Array].
#[wasm_bindgen]
pub struct MarshallGeometry {
    geo_3d: js_sys::Float32Array,
    _triangles: js_sys::Uint32Array
}
#[wasm_bindgen]
impl MarshallGeometry {
    #[wasm_bindgen(getter)]
    pub fn coordinates(&self) -> JsValue {
        JsValue::from(&self.geo_3d)
    }
    #[wasm_bindgen(getter)]
    pub fn triangles(&self) -> JsValue {
        JsValue::from(&self._triangles)
    }
}

//#[wasm_bindgen]
pub type MarshallGeometryTuple = js_sys::Float32Array;//, js_sys::Uint32Array);
unsafe fn marshall_geometry_to_js(geo_3d: Vec<f32>, _triangles: Vec<usize>) -> MarshallGeometry {
    let geo_js = js_sys::Float32Array::view(&geo_3d);
    //remember: u16 is not enough, tiles may have >65536 vertices
    // let tri_u32: [u32; triangles.len()];// = for v in triangles.into_iter() {v as u32}
    // let tri_js = js_sys::Uint32Array::view(&triangles);
    let mut tri_vec: Vec<u32> = vec!();
    for t in _triangles {
        tri_vec.push(t as u32);
    }
    let tri_js = js_sys::Uint32Array::view(&tri_vec);

    //What is a JsValue for?
    //Representing objects owned by JS.
    //Is that what we should be outputting?
    //Perhaps makes sense so we can hand off ownership and allow GC etc as appropriate.
    //JsValue::from(geo_js)
    MarshallGeometry{ geo_3d: geo_js, _triangles: tri_js }
    // JsValue::from(geo_js)
}


struct Contour {
    shape: shapefile::Shape,
    height: f64
}

fn shp_main<R: io::Read + io::Seek>(reader: io::BufReader<R>) -> Result<(Vec<f32>, Vec<usize>), shapefile::Error> {
    //"G:/GIS/OS Terr50/data/su/su67_OST50CONT_20190530.zip"
    let mut contours: Vec<Contour> = Vec::new();

    let mut zip_a = zip::ZipArchive::new(reader)
        .expect("failed to read as ZipArchive");
    
    let types = ["line", "point"];
    for t in types.iter() {
        let shp_p = format!("{}.shp", t);
        let dbf_p = format!("{}.dbf", t);
        
        let shp = zip_util::extract_match_to_memory(&mut zip_a, &shp_p)
            .expect("failed to extract shp");
        let dbf = zip_util::extract_match_to_memory(&mut zip_a, &dbf_p)
            .expect("failed to extract dbf");
        
        let mut reader = shapefile::Reader::new(shp)?;
        reader.add_dbf_source(dbf)?;
        for result in reader.iter_shapes_and_records()? {
            let (shape, record) = result?;
            let height_field = record.get("PROP_VALUE").expect("no PROP_VALUE found");
            let height = match height_field {
                FieldValue::Numeric(n) => n.unwrap(),
                _ => 1./0.
            };
            contours.push(Contour{shape: shape, height: height});
        }
    }

    let mut coordinates: Vec<delaunator::Point> = Vec::new();
    let mut geo_3d: Vec<f32> = Vec::new();
    //perhaps it'd be better to make an array [f32] after 2d work is done and we know how long it needs to be
    for contour in contours.iter() {
        get_points(&contour, &mut coordinates, &mut geo_3d);
    }
    assert_eq!(coordinates.len()*3, geo_3d.len());

    let tri = delaunator::triangulate(&coordinates).expect("No triangulation found.");
    //winding order should be counter-clockwise for front faces - which should mean three & delaunator match
    //but in JS had to reverse winding order for some reason (delaunator.js should also be CCW)
    //If it's necessary to change this, I'll probably do it in combination with casting usize as u32.
    Ok((geo_3d, tri.triangles))
}

fn get_points(contour: &Contour, points: &mut Vec<delaunator::Point>, geo_3d: &mut Vec<f32>) {
    //let geometry = geo_types::Geometry::<f64>::try_from(shape);
    let height: f64 = contour.height;
    
    match &contour.shape {
        shapefile::Shape::Point(p) => {
            points.push(delaunator::Point{ x: p.x, y: p.y });
            geo_3d.append(&mut vec![p.x as f32, p.y as f32, height as f32]);
            // geo_3d.push(p.x);
            // geo_3d.push(p.y);
            // geo_3d.push(height);
        },
        shapefile::Shape::Polyline(line) => {
            for part in line.parts() {
                for p in part {
                    points.push(delaunator::Point{ x: p.x, y: p.y });
                    geo_3d.append(&mut vec![p.x as f32, p.y as f32, height as f32]);
                    // geo_3d.push(p.x);
                    // geo_3d.push(p.y);
                    // geo_3d.push(height);
                }
            }
        }
        _ => {}
    }
}
