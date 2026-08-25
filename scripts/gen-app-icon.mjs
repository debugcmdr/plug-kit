#!/usr/bin/env node
/**
 * 生成 PlugKit 应用图标:蓝渐变圆角底 + 白色 Lucide plug 插头。
 * 插头 path 与 src/components/AppIcon.vue 的 logo 完全同源(SVG stroke 风格),
 * 用 sharp(内嵌 librsvg) 矢量渲染,消除手绘位图的锯齿/瑕疵。
 * 输出多尺寸 PNG + macOS .icns(iconutil) + Windows .ico(Pillow)。
 * 用法: node scripts/gen-app-icon.mjs
 */
import { mkdirSync, rmSync, writeFileSync, readdirSync, statSync, readFileSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
let sharp
try {
  sharp = require('sharp') // 项目内已有则用项目内
} catch {
  // 隔离 workspace 安装回退:优先 SHARP_PATH 环境变量,再试已知开发机路径;
  // 均不可用则明确报错(不再硬编码单用户绝对路径,D-1——换机器/用户即失效)。
  const candidates = [
    process.env.SHARP_PATH,
    '/Users/main/.workbuddy/binaries/node/workspace/node_modules/sharp',
  ].filter(Boolean)
  for (const p of candidates) {
    try { sharp = require(p); break } catch { /* 该候选不可用,试下一个 */ }
  }
  if (!sharp) {
    console.error('缺少 sharp:请在项目安装依赖(npm i -D sharp)或设置 SHARP_PATH 环境变量')
    process.exit(1)
  }
}

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const ICONS_DIR = join(ROOT, 'src-tauri', 'icons')
const SIZES = [16, 32, 64, 128, 256, 512, 1024]

// 与 AppIcon.vue `logo` 分支完全一致的 Lucide plug path(24x24 坐标系)
const PLUG_PATHS = [
  'M12 22v-5',
  'M9 8V2',
  'M15 8V2',
  'M18 8v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4V8Z',
].map(d => `<path d="${d}"/>`).join('')

// 蓝渐变圆角底 + 白色插头。比例严格对齐 Sidebar 品牌 logo:
// 容器 36x36(与 UI 30px 容器等比),插头 24x24 居中 → 占 66.7%(UI 为 20/30),
// 圆角 rx=10.8 → 30%(UI 为 9/30),渐变 135°(#3370ff→#6b9aff,与 UI 同源)。
const SVG = (w, h) => `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 36 36">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#3370ff"/>
      <stop offset="1" stop-color="#6b9aff"/>
    </linearGradient>
  </defs>
  <rect x="0.6" y="0.6" width="34.8" height="34.8" rx="10.8" fill="url(#bg)"/>
  <g transform="translate(6,6)" fill="none" stroke="#ffffff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
    ${PLUG_PATHS}
  </g>
</svg>`

function makeIcns() {
  // iconutil 期望的 iconset 命名(1x/2x)
  const mapping = [
    [16, 'icon_16x16.png'], [32, 'icon_16x16@2x.png'],
    [32, 'icon_32x32.png'], [64, 'icon_32x32@2x.png'],
    [128, 'icon_128x128.png'], [256, 'icon_128x128@2x.png'],
    [256, 'icon_256x256.png'], [512, 'icon_256x256@2x.png'],
    [512, 'icon_512x512.png'], [1024, 'icon_512x512@2x.png'],
  ]
  const seen = {}
  for (const [sz, name] of mapping) seen[name] = sz
  const iconset = join(ICONS_DIR, 'icon.iconset')
  rmSync(iconset, { recursive: true, force: true })
  mkdirSync(iconset)
  for (const [name, sz] of Object.entries(seen)) {
    writeFileSync(join(iconset, name), readFileSync(join(ICONS_DIR, `${sz}.png`)))
  }
  const r = spawnSync('iconutil', ['-c', 'icns', iconset, '-o', join(ICONS_DIR, 'icon.icns')], { encoding: 'utf8' })
  if (r.status !== 0) throw new Error(`iconutil 失败: ${r.stderr}`)
  rmSync(iconset, { recursive: true, force: true })
}

function makeIco() {
  // Pillow 打包多尺寸 ico(sharp 不支持 ICO 输出;用系统 python3,其内置 Pillow)
  const code = `
import os
from PIL import Image
d = ${JSON.stringify(ICONS_DIR)}
sizes = [(16,16),(32,32),(48,48),(64,64),(128,128),(256,256)]
imgs = []
for s, _ in sizes:
    p = os.path.join(d, f'{s}.png')
    if not os.path.exists(p): continue
    imgs.append(Image.open(p).convert('RGBA'))
imgs[-1].save(os.path.join(d, 'icon.ico'), format='ICO', sizes=sizes, append_images=imgs[:-1])
print('ico OK')
`
  const r = spawnSync('/usr/bin/python3', ['-c', code], { encoding: 'utf8' })
  if (r.status !== 0) throw new Error(`Pillow ico 失败: ${r.stderr || r.stdout}`)
}

async function main() {
  // 1. 矢量渲染主图(1024) → 各尺寸(sharp resize 保证小尺寸抗锯齿)
  const master = await sharp(Buffer.from(SVG(1024, 1024))).png().toBuffer()
  for (const s of SIZES) {
    await sharp(master).resize(s, s).png().toFile(join(ICONS_DIR, `${s}.png`))
  }
  await sharp(master).resize(512, 512).png().toFile(join(ICONS_DIR, 'icon.png'))
  console.log(`✓ 生成 PNG: ${SIZES.join(',')} + icon.png(512)`)

  // 2. macOS .icns
  makeIcns()
  console.log('✓ 生成 icon.icns')

  // 3. Windows .ico
  makeIco()
  console.log('✓ 生成 icon.ico')

  for (const f of readdirSync(ICONS_DIR)) {
    console.log(`  ${f}: ${statSync(join(ICONS_DIR, f)).size}B`)
  }
}

main().catch(e => { console.error(e.message); process.exit(1) })
