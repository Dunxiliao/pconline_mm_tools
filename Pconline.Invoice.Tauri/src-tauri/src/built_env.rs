// 这是稳定桩文件：真正的 built_env 内容由 build.rs 写入到 Cargo 的 OUT_DIR
// 并在编译期 include 进来，从而避免 test/prod 并发构建时相互覆盖同一个源文件。
include!(concat!(env!("OUT_DIR"), "/built_env.rs"));
