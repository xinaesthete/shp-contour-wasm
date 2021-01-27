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
pub async fn fetch_shp(url: String) -> Result<JsValue, JsValue> {
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
    let v = io::Cursor::new(d.to_vec());
    let reader = io::BufReader::new(v);
    //compute results (should be similar to already implemented code)
    let _r = shp_main(reader).expect("err");
    //marshall results back into JsValues (preferably SharedArrayBuffers)
    alert(&format!("{} triangles", _r));
    
    Ok(data)
}


struct Contour {
    shape: shapefile::Shape,
    height: f64
}

fn shp_main<R: io::Read + io::Seek>(reader: io::BufReader<R>) -> Result<usize, shapefile::Error> {
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

    //the JS delaunator was unperturbed by getting 3d points as input: element 3 is ignored.
    //I don't expect that we can 'extend' delaunator::Point to have an extra element, 
    //or make another compatible type.
    //So we might end up having redundant data.
    //nb in JS 'numbers' are generally 64bit, but what we ultimately want is a Float32Array ready for handing off to threejs.
    //indeed, we should ideally be able to take a SharedArrayBuffer reference (although there's the issue of not knowing size in advance)
    let mut coordinates: Vec<delaunator::Point> = Vec::new();
    let mut geo_3d: Vec<f64> = Vec::new();
    for contour in contours.iter() {
        get_points(&contour, &mut coordinates, &mut geo_3d);
    }
    assert_eq!(coordinates.len()*3, geo_3d.len());

    let tri = delaunator::triangulate(&coordinates).expect("No triangulation found.");
    Ok(tri.len())
}

/* //JS code to port:
// XXX: nb: I made a very hacky change to shp.js to skip projection.
function getPoints(featureCol) {
    return featureCol.features.flatMap(f => {
        const height = f.properties['PROP_VALUE'];
        switch(f.geometry.type) {
            case "Point":
                return [f.geometry.coordinates.concat(height)];
            case "MultiLineString":
                return f.geometry.coordinates.flatMap(cA=>cA.map(c=>c.concat(height)));
            case "MultiPoint":
            case "LineString":
                return f.geometry.coordinates.map(c => c.concat(height));
            default:
                return [];
        }
    });
}
*/
fn get_points(contour: &Contour, points: &mut Vec<delaunator::Point>, geo_3d: &mut Vec<f64>) {
    //let geometry = geo_types::Geometry::<f64>::try_from(shape);
    let height: f64 = contour.height;
    
    match &contour.shape {
        shapefile::Shape::Point(p) => {
            points.push(delaunator::Point{ x: p.x, y: p.y });
            geo_3d.append(&mut vec![p.x, p.y, height]);
            // geo_3d.push(p.x);
            // geo_3d.push(p.y);
            // geo_3d.push(height);
        },
        shapefile::Shape::Polyline(line) => {
            for part in line.parts() {
                for p in part {
                    points.push(delaunator::Point{ x: p.x, y: p.y });
                    geo_3d.append(&mut vec![p.x, p.y, height]);
                    // geo_3d.push(p.x);
                    // geo_3d.push(p.y);
                    // geo_3d.push(height);
                }
            }
        }
        _ => {}
    }
}
