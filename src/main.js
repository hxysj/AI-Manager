import { createApp } from "vue"
import { ElInput } from "element-plus"
import "element-plus/es/components/input/style/css"
import App from "./App.vue"
import "./styles/app.less"

createApp(App).use(ElInput).mount("#app")
