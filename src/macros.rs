#[macro_export]
macro_rules! spawn_thread {
    ($callback:expr) => {
        std::thread::spawn(move || $callback);
    };
}

#[macro_export]
macro_rules! let_clone {
    ($init:expr, $( $name:ident | $($clone:ident)|* : $ty:ty),*) => {
        $(
            let $name: $ty = $init;
            $(
                let $clone = $name.clone();
            )*
        )*
    };
}
