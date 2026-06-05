import { createApp } from "vue"
import { ElDatePicker, ElInput } from "element-plus"
import "element-plus/es/components/date-picker/style/css"
import "element-plus/es/components/input/style/css"
import App from "./App.vue"
import "./styles/app.less"

createApp(App).use(ElDatePicker).use(ElInput).mount("#app")
