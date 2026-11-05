//! 打印当前编译配置实际生效的变体与编译期开关，供构建脚本核对产物。

fn main() {
    println!("{}", formatforge_lib::flavor::report_json());
}
