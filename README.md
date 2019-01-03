Rust-chat
=========

I wrote this small chat-like pair of server/client in order to learn Rust (borrow-checker,
lifelines, threads, generics). Rust is kinda hard at first glance (lots of syntax) but its
approach in tooling and language is extremely interesting.

Pros:
- No GC but memory-safe
- some good FP constructs (maps for example), nice 
- type-oriented language, no bloated classes or objets
- generics implemented in a nice way (`impl trait`) where you can add traits to a foreign
  type (traits are like interfaces)

Cons:
- language and tooling is evolving at a fast pace; although transitionning between patch/minor version
  is often painless, maintaing a Rocket (for example) project is a pain as it uses
  the `nightly` channel, which in turn often lacks some tooling randomly (`rls` mainly)
- The tooling, primarely `rls` (Rust Language Server), is young and somehow the types-on-hover
  doesn't work great (compared to OCaml, ReasonML or even Typescript). Because of the trait stuff,
  the RLS often doesn't give which methods can be called or stuff like that.


To test this small POC:

    cargo run -- server 9000
    cargo run -- client 127.0.0.1 9000
    cargo run -- client 127.0.0.1 9000

[![screencast asciinema](https://user-images.githubusercontent.com/2195781/50637017-943bda80-0f58-11e9-80e0-882f71bbc118.gif)](https://asciinema.org/a/k63SVx2a2ATY9npFOnSvujTyL)

