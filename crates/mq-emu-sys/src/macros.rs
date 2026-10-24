/// Declares the imports from one DLL and, alongside them, a list of every
/// declared symbol name. The list lets tests check the declarations against
/// the DLL's real export table, so the two can't drift apart unnoticed.
macro_rules! imports {
    (
        dll: $dll:literal,
        names: $names:ident,
        functions {
            $( $(#[$fattr:meta])* pub fn $fname:ident ( $($fargs:tt)* ) $(-> $fret:ty)? ; )*
        }
        statics {
            $( $(#[$sattr:meta])* pub static mut $sname:ident : $sty:ty ; )*
        }
    ) => {
        #[link(name = $dll)]
        unsafe extern "C" {
            $( $(#[$fattr])* pub fn $fname( $($fargs)* ) $(-> $fret)?; )*
            $( $(#[$sattr])* pub static mut $sname: $sty; )*
        }

        /// Every symbol name declared in this module, as exported by the DLL.
        #[doc(hidden)]
        pub const $names: &[&str] = &[ $(stringify!($fname),)* $(stringify!($sname),)* ];
    };
}

#[cfg(test)]
mod tests {
    #![allow(dead_code)]

    use core::ffi::c_char;

    // The declarations are never referenced, so no MQ2Main.dll import is generated;
    // the .lib only has to exist for the linker.
    imports! {
        dll: "MQ2Main",
        names: TEST_IMPORTS,
        functions {
            /// Doc comments and variadics must pass through.
            pub fn VariadicFn(format: *const c_char, ...);
            pub fn ReturnsValue(a: i32) -> bool;
        }
        statics {
            pub static mut SomeGlobal: [c_char; 4];
        }
    }

    imports! {
        dll: "MQ2Main",
        names: EMPTY_IMPORTS,
        functions {}
        statics {}
    }

    #[test]
    fn names_list_every_function_then_static() {
        assert_eq!(TEST_IMPORTS, ["VariadicFn", "ReturnsValue", "SomeGlobal"]);
    }

    #[test]
    fn empty_imports_have_no_names() {
        assert!(EMPTY_IMPORTS.is_empty());
    }
}
