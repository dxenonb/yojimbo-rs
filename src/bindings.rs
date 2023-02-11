#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::marker::PhantomData;

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

pub struct NetcodeAddress<'a> {
    raw: *const netcode_address_t,
    phantom: PhantomData<&'a ()>,
}

impl<'a> NetcodeAddress<'a> {
    pub unsafe fn new(raw: *const netcode_address_t) -> NetcodeAddress<'a> {
        assert!(!raw.is_null());
        NetcodeAddress {
            raw,
            phantom: PhantomData,
        }
    }

    pub fn is_ipv4(&self) -> bool {
        unsafe { (*self.raw).type_ == NETCODE_ADDRESS_IPV4 as _ }
    }

    pub fn is_ipv6(&self) -> bool {
        unsafe { (*self.raw).type_ == NETCODE_ADDRESS_IPV6 as _ }
    }

    pub fn ipv4(&self) -> Option<&[u8; 4]> {
        if self.is_ipv4() {
            unsafe { Some(&(*self.raw).data.ipv4) }
        } else {
            None
        }
    }

    pub fn ipv6(&self) -> Option<&[u16; 8]> {
        if self.is_ipv4() {
            unsafe { Some(&(*self.raw).data.ipv6) }
        } else {
            None
        }
    }

    pub fn port(&self) -> u16 {
        unsafe { (*self.raw).port }
    }
}

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
