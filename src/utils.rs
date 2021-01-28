use std::io;
use std::io::*;
use zip::*;
use zip::result::{ZipResult, ZipError};
use wasm_bindgen::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Window, WorkerGlobalScope, Request};
use js_sys::{Promise};

/// find a file ending with the given character string in the archive, 
/// copy into memory (no longer requiring ownership of archive) and return a cursor to read that memory.
pub fn extract_match_to_memory<R: Read + io::Seek>(archive: &mut ZipArchive<R>, ending: &str) -> ZipResult<io::Cursor<Vec<u8>>> {
    //XXXXX::: NO!!!! order of file_names() does not correspond to by_index().
    // let file_number = archive.file_names().position(|f| f.ends_with(ending)});
    
    for file_number in 0..archive.len() {
        if let Ok(mut file) = archive.by_index(file_number) {
            if file.name().ends_with(ending) {
                let mut buffer: Vec<u8> = vec![];
                let _bytes_read = file.read_to_end(&mut buffer)?;
                //let arr: &[u8] = &buffer; //works because Vec<T> implements AsRef<[T]>
                return Ok(io::Cursor::new(buffer));
            }
        }
    }
    Err(ZipError::Io(Error::new(ErrorKind::NotFound, 
        "No index found for file name with specified ending.")))
}

/// from default boilerplate
pub fn set_panic_hook() {
    // When the `console_error_panic_hook` feature is enabled, we can call the
    // `set_panic_hook` function at least once during initialization, and then
    // we will get better error messages if our code ever panics.
    //
    // For more details see
    // https://github.com/rustwasm/console_error_panic_hook#readme
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}


//--------------------------
//I want a WorkerGlobalScope here in place of window.
//made relevant change to Cargo.toml, looks like I'll need something more elaborate to get
//WorkerGlobalScope
//I only want it to work in worker, but I suppose I could follow this approach
//https://github.com/rustwasm/gloo/blob/994d683f64fc04380f495fafb356521f346eff5f/crates/timers/src/callback.rs
//to make it still work in window & even also hypothetically extend to work in node.

pub fn fetch_with_request(request: &Request) -> Promise {
    GLOBAL.with(|global| global.fetch_with_request(&request))
}

thread_local! {
    static GLOBAL: WindowOrWorker = WindowOrWorker::new();
}
enum WindowOrWorker {
    Window(Window),
    Worker(WorkerGlobalScope),
}

impl WindowOrWorker {
    fn new() -> Self {
        #[wasm_bindgen]
        extern "C" {
            type Global;

            #[wasm_bindgen(method, getter, js_name = Window)]
            fn window(this: &Global) -> JsValue;

            #[wasm_bindgen(method, getter, js_name = WorkerGlobalScope)]
            fn worker(this: &Global) -> JsValue;
        }

        let global: Global = js_sys::global().unchecked_into();

        if !global.window().is_undefined() {
            Self::Window(global.unchecked_into())
        } else if !global.worker().is_undefined() {
            Self::Worker(global.unchecked_into())
        } else {
            panic!("Only supported in a browser or web worker");
        }
    }
}

macro_rules! impl_window_or_worker {
    ($(fn $name:ident($($par_name:ident: $par_type:ty),*)$( -> $return:ty)?;)+) => {
        impl WindowOrWorker {
            $(
                fn $name(&self, $($par_name: $par_type),*)$( -> $return)? {
                    match self {
                        Self::Window(window) => window.$name($($par_name),*),
                        Self::Worker(worker) => worker.$name($($par_name),*),
                    }
                }
            )+
        }
    };
}

impl_window_or_worker! {
    fn fetch_with_request(request: &Request) -> Promise;
    // fn set_timeout_with_callback_and_timeout_and_arguments_0(handler: &Function, timeout: i32) -> Result<i32, JsValue>;
    // fn set_interval_with_callback_and_timeout_and_arguments_0(handler: &Function, timeout: i32) -> Result<i32, JsValue>;
    // fn clear_timeout_with_handle(handle: i32);
    // fn clear_interval_with_handle(handle: i32);
}

//--------------------------------------------