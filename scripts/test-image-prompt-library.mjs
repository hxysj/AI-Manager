import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import { parse, compileScript, compileTemplate, compileStyleAsync } from '@vue/compiler-sfc'

const path = resolve('src/features/tools/components/ImagePromptLibrary.vue')
const source = await readFile(path, 'utf8')
const { descriptor, errors } = parse(source)
assert.deepEqual(errors, [])

const script = compileScript(descriptor, { id: path })
assert.ok(script.content.length > 0)

const template = compileTemplate({
  source: descriptor.template.content,
  filename: path,
  id: path,
  compilerOptions: { bindingMetadata: script.bindings }
})
assert.deepEqual(template.errors, [])

const style = await compileStyleAsync({
  source: descriptor.styles[0].content,
  filename: path,
  id: path,
  preprocessLang: 'less'
})
assert.deepEqual(style.errors, [])

console.log('ImagePromptLibrary.vue SFC parse & compile test passed successfully!')
