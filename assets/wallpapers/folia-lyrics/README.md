# Folia 歌词可视化 (wallpaper pack)

pulse-ring scene 包：把 folia 的全部 11 个歌词可视化模式渲染为壁纸层之上的透明 overlay。

## 清单
- `project.json` — manifest（type:"scene"，指向已构建的 folia-web 包）
- `file` 用相对路径 `../../../folia-wallpaper/dist/index.html` 指回工程内已构建产物

## 重建 folia bundle
当 `folia-wallpaper/src` 改动后，需重新构建：
```
cd folia-wallpaper && npm install && npm run build
```
产物 `dist/index.html`（相对路径 base，Electron 离屏友好）即被本包引用。

## 启用
`config` 中设置 `scene_wallpaper = "assets/wallpapers/folia-lyrics"`（或壁纸库名 `folia-lyrics`）。

## 选模式
- 全局：preload 从 `config.params.visualizerMode` 设 `window.__FOLIA_MODE__`
- 临时：page URL 加 `?mode=sonnet|classic|cadenza|partita|fume|claddagh|cappella|tilt|monet|diorama|pendolo`
- 默认：`classic`
