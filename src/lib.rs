mod utils;

use std::io;
// use std::io::*; //woe betide
// use std::time::{Duration, SystemTime, UNIX_EPOCH};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{DedicatedWorkerGlobalScope, Request, RequestInit, RequestMode, Response};
use shapefile::dbase::*;

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen(start)]
pub fn main() -> Result<(), JsValue> {
    utils::set_panic_hook();
    Ok(())
}

#[wasm_bindgen]
pub async fn shp_contour_worker(url: String) -> Result<JsValue, JsValue> {
    let worker: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let result = fetch_shp(url).await?;
    worker.post_message(&result.into())?;
    Ok(JsValue::from(true))
}
/// Retrieve a shapefile in zip format and return mesh data asynchronously.
/// (assumes that input is similar to OS Terr 50 data:
/// each zip having a *line.shp, *line.dbf, *point.shp, *point.dbf
/// it'll probably fail somewhat ungraciously with data from any other source)
#[wasm_bindgen]
pub async fn fetch_shp(url: String) -> Result<MarshallGeometry, JsValue> {
    let mut opts = RequestInit::new();
    opts.method("GET");
    opts.mode(RequestMode::Cors); //probably shouldn't need CORS actually
    
    let request = Request::new_with_str_and_init(&url, &opts)?;
    request.headers().set("Accept", "application/zip")?;
    
    let resp_value = JsFuture::from(utils::fetch_with_request(&request)).await?;
    assert!(resp_value.is_instance_of::<Response>());
    let resp: Response = resp_value.dyn_into().unwrap();
    //deal with 404 etc.
    if !resp.ok() {
        //why do I need to be explicit about coercing type here, but "?"" works?
        return Err(utils::MyError::BadResponse().into());
    }
    
    let data = JsFuture::from(resp.array_buffer()?).await?;
    //get data into a form readable by other rust methods
    let d = js_sys::Uint8Array::new(&data);
    let v = io::Cursor::new(d.to_vec());
    let reader = io::BufReader::new(v);
    //compute results (should be similar to already implemented code)
    let (geo_3d, triangles) = shp_main(reader)?;
    
    //marshall results back into JsValues (preferably SharedArrayBuffers)
    
    //this is unsafe because we make 'views' of our data in JS typed arrays
    //(as yet, not SharedArrayBuffer, which could be even more unsafe?)
    //DOMException: Failed to execute 'postMessage' on 'DedicatedWorkerGlobalScope': 
    //An ArrayBuffer is detached and could not be cloned.
    // --- ^^^ see shp_contour_worker above for version that calls postMessage directly ^^^
    // could also hook into onmessage to minimise JS boilerplate
    // potentially manage worker threads from Rust as well.
    
    // utils::console_log!("{} with {}", url, triangles.len()/3);
    // web_sys::console::log(format!("{} with {}", url, triangles.len()/3));
    unsafe {
        Ok(marshall_geometry_to_js(geo_3d, triangles))
    }
}

//wasm_bindgen types cannot have lifetime specifiers
//also seem to be pretty limited in available types, lots of 'copy is not specified' complaints on pub fields
//may need getters https://github.com/rustwasm/wasm-bindgen/issues/439
#[wasm_bindgen]
pub struct MarshallGeometry {
    geo_3d: js_sys::Float32Array,
    _triangles: js_sys::Uint32Array,
    // could compute normals and add them here as well
    // (may consider doing that in JS version as well)
    // pub computeTime: f64
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

unsafe fn marshall_geometry_to_js(geo_3d: Vec<f32>, triangles: Vec<usize>) -> MarshallGeometry {
    // let geo = geo_3d.into_boxed_slice();
    //--- if I make views based on into_boxed_slice(), I get odd bits of geometry
    //     : the js representation is Array rather than TypedArray
    //    if I don't, I get occasional errors in JS:
    //index.js:1 DOMException: Failed to execute 'postMessage' on 'DedicatedWorkerGlobalScope': 
    //An ArrayBuffer is detached and could not be cloned.
    //---- BUT::: the graphics look ok.

    //https://github.com/rustwasm/wasm-bindgen/blob/master/examples/raytrace-parallel/src/lib.rs
    //let mem = wasm_bindgen::memory().unchecked_into::<WebAssembly::Memory>();
    let geo_js = js_sys::Float32Array::view(&geo_3d);
    //remember: u16 is not always enough, tiles may have >65536 vertices
    //maybe I could do something quicker here, meh.
    let mut tri_vec: Vec<u32> = Vec::with_capacity(triangles.len());
    //reversing winding as we go (or not)
    // for i in 0..triangles.len()/3 {
    //     tri_vec.push(triangles[i*3 + 2] as u32);
    //     tri_vec.push(triangles[i*3 + 1] as u32);
    //     tri_vec.push(triangles[i*3 + 0] as u32);
    // }
    for t in triangles {
        tri_vec.push(t as u32);
    }
    let tri_js = js_sys::Uint32Array::view(&tri_vec);

    MarshallGeometry{ geo_3d: geo_js, _triangles: tri_js }
}


struct Contour {
    shape: shapefile::Shape,
    height: f64 //could make this f32 sooner rather than later
}


fn shp_main<R: io::Read + io::Seek>(reader: io::BufReader<R>) -> Result<(Vec<f32>, Vec<usize>), utils::MyError> {
    let mut contours: Vec<Contour> = Vec::new();

    let mut zip_a = zip::ZipArchive::new(reader)?; //can happen: InvalidArchive("Could not find central directory end")
    
    let types = ["line", "point"];
    for t in types.iter() {
        let shp_p = format!("{}.shp", t);
        let dbf_p = format!("{}.dbf", t);
        
        let shp = utils::extract_match_to_memory(&mut zip_a, &shp_p)?;
        let dbf = utils::extract_match_to_memory(&mut zip_a, &dbf_p)?;
        
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

    // let tri = match delaunator::triangulate(&coordinates) {
    //     None => Err(utils::MyError::NoTriangulation()),
    //     Some(t) => t
    // };
    let tri = delaunator::triangulate(&coordinates).unwrap();
    
    //winding order should be counter-clockwise for front faces - which should mean three & delaunator match
    //but in JS had to reverse winding order for some reason (delaunator.js should also be CCW)
    //If it's necessary to change this, I'll probably do it in combination with casting usize as u32.
    Ok((geo_3d, tri.triangles))
}

fn get_points(contour: &Contour, points: &mut Vec<delaunator::Point>, geo_3d: &mut Vec<f32>) {
    let height: f64 = contour.height;
    
    match &contour.shape {
        shapefile::Shape::Point(p) => {
            points.push(delaunator::Point{ x: p.x, y: p.y });
            geo_3d.append(&mut vec![p.x as f32, p.y as f32, height as f32]);
        },
        shapefile::Shape::Polyline(line) => {
            for part in line.parts() {
                for p in part {
                    points.push(delaunator::Point{ x: p.x, y: p.y });
                    geo_3d.append(&mut vec![p.x as f32, p.y as f32, height as f32]);
                }
            }
        }
        _ => {}
    }
}
