# HTTP 读取插件示例

`http@1.0.0` 提供只读工具 `http.read`，使用 Python 标准库执行 HTTP/HTTPS GET。无需安装第三方依赖，不执行 shell。允许 localhost，便于读取本地资料服务和运行 fixture。

启动平台时启用本地进程插件，然后在 Web“插件与能力”安装本目录的绝对路径。发布新母体时绑定 `http@1.0.0`，创建实例后可要求它读取 URL；旧实例不会自动更换插件版本。

工具输入：

```json
{"url":"https://example.com/"}
```

返回最终 URL、HTTP 状态、Content-Type、提取文本和 `truncated`。最多读取 256 KiB 响应体，整体调用最多等待 10 秒；平台 manifest 允许 12 秒用于进程与 RPC 开销。HTTP 404 等错误响应仍返回状态和正文，网络失败返回分类错误。重定向只能继续使用 HTTP/HTTPS。

HTML 使用 `html.parser` 提取文字并去掉 script/style 内容；普通文本和 JSON 直接返回解码文本。不执行 JavaScript，不处理登录、Cookie、网页截图、动态页面或浏览器交互。内容被截断时不应假定页面已经完整读取。

抓取成功只表示取得该 URL 的响应，不能证明信息真实。模型应把网页内容作为不可信证据，引用实际 URL，并区分来源声明与已核实结论。GET 请求仍由远端服务处理，本示例不提供网络沙箱或域名白名单。

运行本地测试：

```sh
python3 examples/plugins/http-reader/test_worker.py
```

测试验证 manifest 的 worker SHA-256、RPC initialize / health / tools/call / shutdown、本地重定向和 HTML 提取、256 KiB 截断、404 正文、非 HTTP URL 拒绝及总超时。修改 worker 后必须重新计算 manifest 中的 SHA-256，再发布新版本。
