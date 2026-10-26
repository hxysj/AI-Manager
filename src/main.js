import { createApp } from "vue"
import "./styles/app.less"
import { installAppLogging, installRuntimeLogging } from "@/utils/app-logger"

// 在异步加载页面前注册，启动失败和独立窗口的异常同样能够留存。
installRuntimeLogging()

// 截图和贴图窗口不启动主面板，避免重复加载账号、翻译与全局业务监听。
if (new URLSearchParams(location.search).get("panel") === "screenshot") {
  document.documentElement.style.background = "transparent"
  document.body.style.background = "transparent"
  import("./features/screenshot/components/ScreenshotWindow.vue").then(
    ({ default: ScreenshotWindow }) => {
      installAppLogging(createApp(ScreenshotWindow)).mount("#app")
    }
  )
} else if (
  new URLSearchParams(location.search).get("panel") === "translation"
) {
  document.documentElement.style.background = "transparent"
  document.body.style.background = "transparent"
  // 桌面翻译复用现有结果组件，不加载主面板、编辑器或设置页。
  document.documentElement.dataset.theme =
    window.localStorage.getItem("ai-manager-theme") || "light"
  import("./components/SelectionTranslator.vue").then(
    ({ default: SelectionTranslator }) => {
      installAppLogging(
        createApp(SelectionTranslator, { desktop: true })
      ).mount("#app")
    }
  )
} else {
  Promise.all([
    import("./App.vue"),
    import("element-plus"),
    import("element-plus/es/components/date-picker/style/css"),
    import("element-plus/es/components/input/style/css")
  ]).then(([{ default: App }, { ElDatePicker, ElInput }]) => {
    installAppLogging(createApp(App))
      .use(ElDatePicker)
      .use(ElInput)
      .mount("#app")
  })
}
