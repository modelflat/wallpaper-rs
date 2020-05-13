# wallpaper-rs

The goal of `wallpaper-rs` is to provide a portable, single-executable, not-very-feature-rich application which can turn regular windows 
into wallpapers (i.e. place windows behind desktop icons).

The heart of this app is a Rust rewrite of [WeebP](https://github.com/Francesco149/weebp/) (which is originally written in C), 
with a simple [web-view](https://github.com/Boscop/web-view) UI on top.

It should work on Windows 10, and *might* work on Windows 8/8.1. Windows versions below 8 are not supported (as opposed to weebp,
which will probably work on Windows 7 and below).

Current version is MVP, but `wallpaper-rs` still lacks many of the features of WeebP, and the code is far from clean and reusable 
(there is mostly unsafe code, most functions return `bool`s instead of proper `Result`s, etc.). I might clean it up eventually though.
