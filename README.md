# encoding-converter

编码转换器：在字符编码 / 文本格式之间转换的小工具（开发中）。

## 功能

- 规划中：字符编码互转（UTF-8 / GBK / UTF-16 等）、常见文本格式转换。
- 当前状态：仓库初始化完成，功能代码待添加。

## 打开方式

- 计划采用纯前端方案：实现后直接用浏览器打开 `index.html` 即可运行，无需搭建服务。

## 本地存储

- 计划使用浏览器 `localStorage` 保存用户设置，不依赖外部后端接口。

## 开发约定

- AI 编码遵循 [AGENTS.md](./AGENTS.md)。
- 提交信息使用约定式提交：`feat/fix/docs/style/refactor/test/chore(scope): message`。

## 自动推送

- 本机已安装 `.git/hooks/post-commit`：每次提交后自动通过 SSH `git push` 到 `origin`。
- 推送日志见 `.git/autopush.log`；不需要自动推送时，删除该钩子文件即可。

## 远程仓库

- GitHub：https://github.com/AlanLumier/encoding-converter
- origin：`git@github.com:AlanLumier/encoding-converter.git`（SSH）
