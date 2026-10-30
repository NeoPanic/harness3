# 控制台源码

`src/app` 保存路由、认证、共享布局和样式，`src/features` 按功能组织页面；`index.html` 为入口。沿用原生 HTML/CSS/JavaScript，无需 Node 开发服务器或打包依赖。静态文件由平台 HTTP 入口在编译时嵌入，资源 URL 保持不变；修改后需重新构建并重启服务。

在仓库根目录执行 `node --test scripts/test-*.cjs` 检查前端逻辑。
