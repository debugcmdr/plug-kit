#!/usr/bin/env node
// 打包前清理 plugins/ 下的 Python 缓存(__pycache__/*.pyc)。
// 用于 beforeBuildCommand:tauri bundle.resources 把整个 plugins/ 打进安装包,
// 本地磁盘若残留 pyc(运行插件 CLI 时由 Python 生成)会一并打包,纯垃圾体积。
// 跨平台:不依赖 find/rm 的 shell 差异。
import { rmSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(fileURLToPath(new URL('.', import.meta.url)), '..', 'plugins')
let removed = 0

function walk(dir) {
  let entries
  try {
    entries = readdirSync(dir)
  } catch {
    return
  }
  for (const name of entries) {
    const p = join(dir, name)
    let st
    try {
      st = statSync(p)
    } catch {
      continue
    }
    if (!st.isDirectory()) continue
    if (name === '__pycache__') {
      rmSync(p, { recursive: true, force: true })
      removed++
    } else {
      walk(p)
    }
  }
}

walk(root)
if (removed > 0) console.log(`[clean-plugin-cache] removed ${removed} __pycache__ dir(s)`)
