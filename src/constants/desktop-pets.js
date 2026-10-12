// 同一素材目录是一只桌宠，manifest 中的 WebP 是它的不同动作。
const manifests = import.meta.glob("/src/assets/pets/*/manifest.json", {
  eager: true,
  import: "default"
})
const spriteConfigs = import.meta.glob(
  "/src/assets/pets/*/sprite_config.json",
  {
    eager: true,
    import: "default"
  }
)
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
  work: "工作",
  work_laptop: "用电脑",
  exercise: "锻炼",
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
    const config = spriteConfigs[`${directory}/sprite_config.json`]
    const animations = Object.entries(manifest)
      .map(([action, item]) => {
        // 裁切坐标使用素材配置，播放时保持同一动作的画布尺寸稳定。
        const sprite = config.actions[action]
        // 每个动作独立配置帧间隔，轮换周期也随该动作的速度计算。
        const frameDuration = sprite.frameDurationMs
        return {
          id: action,
          name: animationNames[action] || action.replace(/[_-]/g, " "),
          url: images[`${directory}/${item.file}`],
          sheetWidth: sprite.sheetWidth,
          sheetHeight: sprite.sheetHeight,
          staticFrame: sprite.staticFrame,
          frames: sprite.frameRects,
          frameWidth:
            Math.max(...sprite.frameRects.map((frame) => frame.width)) +
            (sprite.staticFrame ? sprite.staticFrame.width : 0),
          frameHeight: Math.max(
            ...sprite.frameRects.map((frame) => frame.height)
          ),
          frameDuration,
          duration: sprite.frameCount * frameDuration
        }
      })
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
