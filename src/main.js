import { createApp } from "vue"
import { ElDatePicker, ElInput } from "element-plus"
import "element-plus/es/components/date-picker/style/css"
import "element-plus/es/components/input/style/css"
import App from "./App.vue"
import "./styles/app.less"

// 截图和贴图窗口不启动主面板，避免重复加载账号、翻译与全局业务监听。
if (new URLSearchParams(location.search).get("panel") === "screenshot") {
  document.documentElement.style.background = "transparent"
  document.body.style.background = "transparent"
  import("./features/tools/screenshot/components/ScreenshotWindow.vue").then(
    ({ default: ScreenshotWindow }) => {
      createApp(ScreenshotWindow).mount("#app")
    }
  )
} else {
  createApp(App).use(ElDatePicker).use(ElInput).mount("#app")
}
