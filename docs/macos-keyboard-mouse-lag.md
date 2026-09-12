# macOS 键盘控制鼠标卡顿排查（2026-09-12）

键盘控制鼠标的移动链路需要在后台保持及时响应，并正确处理调度延迟、屏幕边缘和输入中断。以下记录可复现的问题、修复方式及验证边界；尚未完成实际卡顿场景下修复前后的连续对照，不能据此认定某个假设是所有长时间使用卡顿的唯一原因。

## 现场证据

- 运行版本 0.1.2；对已运行约 6 小时 49 分钟的进程做了 5 秒采样。采样报告的 physical footprint 为 57.9 MB，峰值为 115.1 MB。没有观察到主线程持续阻塞；单次采样不能排除间歇性阻塞或缓慢泄漏。
- 键盘事件在独立 CGEventTap 线程中进入 MappingEngine。方向变化写入一个共享状态，独立鼠标线程产生约 120 Hz 的移动事件。鼠标移动不经过前端渲染、Tauri 事件队列或 Alt-Tab 工作队列。
- 原来的移动线程即使空闲也每约 8.33 ms 醒来。两个输入线程均未设置交互 QoS，移动过程中也没有 NSProcessInfo 活动声明。
- macOS 的 App Nap 可以降低后台进程优先级和定时器频率，这是合理的嫌疑，但本次没有捕获到该进程处于 App Nap 的直接证据。[Apple：App Nap](https://developer.apple.com/library/archive/documentation/Performance/Conceptual/power_efficiency_guidelines_osx/AppNap.html)
- 另外运行了只观察 Yorling 自带标记鼠标事件的 30 秒采样，该窗口内没有收到移动事件，因此没有可用于比较的真实移动帧间隔数据。

## 可确认的问题和修复

| 问题 | 原行为 | 修复 |
| --- | --- | --- |
| 调度延迟被换算成大段位移 | 回归测试先在旧实现失败：模拟 2 秒延迟，单帧跳了 2800 像素 | 单帧计算最多使用 16.666 ms；超过 100 ms 的停顿清空速度、重新读取实际位置；不补发错过的帧 |
| 屏幕边缘坐标持续累加 | 系统光标停在边缘，但软件内部位置继续向屏幕外增长，转向时需要先抵消这段距离 | 按真实显示器区域约束并保存内部坐标，支持负坐标和错位排列，活动中每秒刷新显示器列表 |
| 松开后很快重新按下 | 如果两次变化都发生在刷新间隔内，工作线程可能看不到中间的停止，沿用上次的速度和位置 | 用会话计数保留停止和重启的信息，同时仍只保存最新方向，不排队旧移动 |
| 空闲轮询及后台调度 | 无操作仍不断唤醒，输入线程调度意图未声明 | 空闲条件变量等待；按键变化主动唤醒；输入线程使用交互 QoS；移动期间持有活动令牌，空闲和退出自动结束 |
| 监听中断后可能丢失松键 | 仅重新启用 CGEventTap，原移动方向可能残留 | 中断时停止移动并取消鼠标模式，避免继续执行过期的按键状态 |

活动使用 `UserInitiatedAllowingIdleSystemSleep`，不会要求系统或显示器保持唤醒。按 Apple 建议成对调用 begin/end，并在输入互斥锁外执行 Foundation 调用。[Apple：活动管理](https://developer.apple.com/library/archive/documentation/Performance/Conceptual/power_efficiency_guidelines_osx/PrioritizeWorkAtTheAppLevel.html)、[Apple：交互 QoS](https://developer.apple.com/documentation/dispatch/dispatchqos/qosclass-swift.enum/userinteractive)

鼠标线程每帧设有 autorelease pool，及时释放系统框架可能生成的临时对象；这属于资源管理修正，不代表已证明原版本存在这类泄漏。线程启停互斥并 join，防止快速重启重叠工作线程。

## 验证

- 原实现的延迟回归测试失败，修复后通过。
- `cargo test --workspace`：268 项通过，其中引擎 108 项、macOS 17 项。
- 覆盖持续约 6.94 小时的数学模拟（300 万帧）、10 万次顶住边缘、跨屏/负坐标/屏幕移除、亚像素精度、快速停止再启动、跳过错过的帧、最新方向覆盖、真实工作线程空闲/唤醒/退出、监听中断。
- 数学模拟是快速执行的状态测试，不是让 GUI 在真实系统负载下跑了 7 小时。工作线程测试使用模拟输出，不会移动桌面鼠标。
- `pnpm test`：181 项通过。
- 前端 TypeScript/Vite 生产构建通过；macOS release 应用完成构建，`codesign --verify --deep --strict` 验证通过。启动后的采样确认新鼠标线程在空闲条件变量上等待。

## 手动验证与后续诊断

重点观察隐藏主窗口后的持续移动、顶住屏幕边缘再转向、快速松按，以及长时间使用后的手感。测试时使用重新构建的应用，避免旧进程仍在运行而测到旧实现。

若仍有停顿，在实际发生时采样才有诊断价值。新实现会记录超过 100 ms 的活动期间调度间隔，最多每 10 秒一条。需要日志时，退出旧进程后从仓库根目录启动已构建的应用：

```sh
RUST_LOG=yorling_platform_macos=warn ./target/release/bundle/macos/Yorling.app/Contents/MacOS/yorling 2> yorling-input.log
```

`Mouse motion scheduling gap` 表示鼠标线程调度/前帧工作曾延迟；`CGEventTap disabled` 表示键盘监听被系统暂停。日志不包含按键内容。若两种日志都没有但仍卡顿，需要继续比较实际发出的鼠标事件与 WindowServer 显示响应，并检查其他全局输入工具；不要据此直接归因于内存泄漏。
