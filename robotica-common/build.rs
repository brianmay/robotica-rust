fn main() {
    prost_build::compile_protos(&["src/protos/websocket.proto"], &["src/"]).unwrap();
}
