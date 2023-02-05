#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

#[macro_export]
macro_rules! gf_init_default {
    ($strukt:ty, $default_function:ident) => {
        unsafe {
            let mut obj: std::mem::MaybeUninit<$strukt> = std::mem::MaybeUninit::uninit();
            $default_function(obj.as_mut_ptr());
            obj.assume_init()
        }
    };
}

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
