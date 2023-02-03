fn main() {
    #[cfg(feature = "websockets")]
    prost_build::compile_protos(&["src/protos/websocket.proto"], &["src/"]).unwrap();
}
