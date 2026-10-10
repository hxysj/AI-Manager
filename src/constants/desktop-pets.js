// 同一素材目录是一只桌宠，manifest 中的 WebP 是它的不同动作。
const manifests = import.meta.glob("/src/assets/pets/*/manifest.json", {
  eager: true,
  import: "default"
})
const images = import.meta.glob("/src/assets/pets/*/*.webp", {
  eager: true,
  query: "?url",
  import: "default"
})

const animationNames = {
  idle: "待机",
  walk: "行走",
  greeting: "打招呼",
  jump: "跳跃",
  read_book: "看书",
  use_phone: "玩手机",
  sleep: "睡觉",
  think: "思考",
  work_laptop: "用电脑",
  salute: "敬礼",
  steal: "偷东西",
  peek: "偷看",
  run: "奔跑",
  wake_up: "醒来"
}

export const desktopPets = Object.entries(manifests)
  .sort(([left], [right]) => left.localeCompare(right))
  .map(([path, manifest]) => {
    const directory = path.slice(0, path.lastIndexOf("/"))
    const id = directory.split("/").at(-1)
    const animations = Object.entries(manifest)
      .map(([action, item]) => ({
        id: action,
        name: animationNames[action] || action.replace(/[_-]/g, " "),
        url: images[`${directory}/${item.file}`],
        duration: item.frame_count * item.frame_duration_ms
      }))
      .filter((animation) => animation.url)
    return {
      id,
      name: id === "monkey_animations" ? "小猴子" : id.replace(/[_-]/g, " "),
      animations
    }
  })
  .filter((pet) => pet.animations.length)

// 上次选择失效或首次启用时，统一回到目录中的第一只桌宠。
export function getDesktopPet(id) {
  return desktopPets.find((pet) => pet.id === id) || desktopPets[0] || null
}
