# 源码参考

本书的页面可以用三种方式指向源码。

**API 参考。** Rust 部分的每个模块、类型和函数（包括私有的），由 `rustdoc`
直接从源码生成，和本书发布在一起：[`shellder` crate](../api/shellder/index.html)。
页面可以链接到其中的某一项，例如
[`core::pty`](../api/shellder/core/pty/index.html)：控制终端，以及回答 ssh 提问的
读取线程。API 参考只有英文。

**原样嵌入一段源码。** 页面可以显示仓库里某个文件的若干行，在构建本书时读取，
所以页面不会落后于代码。下面是被锁定的主机重新连接所遵循的规则，来自
`src-tauri/src/core/reconnect.rs`：

```rust,ignore
{{#include ../../../src-tauri/src/core/reconnect.rs:1:24}}
```

**链接到文件。** `ui/` 下各页面的脚本没有自己的参考文档，直接链接到文件：
[`ui/console.js`](https://github.com/Phantom1003/Shellder/blob/main/ui/console.js)。

仓库的结构、测试和构建方法见
[`AGENTS.md`](https://github.com/Phantom1003/Shellder/blob/main/AGENTS.md)。
