import { cp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const desktopRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(desktopRoot, '..');
const sourceDist = path.join(repoRoot, 'dist');
const targetDist = path.join(desktopRoot, 'dist-desktop');

function stripRemoteHeadTags(html) {
  return html
    .replace(/^\s*<link rel="canonical"[\s\S]*?\/>\s*$/gm, '')
    .replace(/^\s*<link rel="alternate" hreflang="[^"]+"[\s\S]*?\/>\s*$/gm, '')
    .replace(/^\s*<link rel="preconnect" href="https:\/\/fonts\.googleapis\.com" \/\>\s*$/gm, '')
    .replace(/^\s*<link rel="preconnect" href="https:\/\/fonts\.gstatic\.com" crossorigin \/\>\s*$/gm, '')
    .replace(/^\s*<link href="https:\/\/fonts\.googleapis\.com\/css2[\s\S]*?rel="stylesheet"\s*\/>\s*$/gm, '')
    .replace(/<meta property="og:url" content="[^"]*" \/>/g, '<meta property="og:url" content="app://pixlab.local/" />')
    .replace(/<meta property="og:type" content="website" \/>/g, '<meta property="og:type" content="product" />');
}

async function patchHtmlFiles(rootDir) {
  const queue = [rootDir];
  while (queue.length > 0) {
    const current = queue.pop();
    const entries = await (await import('node:fs/promises')).readdir(current, { withFileTypes: true });
    for (const entry of entries) {
      const nextPath = path.join(current, entry.name);
      if (entry.isDirectory()) {
        queue.push(nextPath);
        continue;
      }
      if (!entry.name.endsWith('.html')) continue;
      const html = await readFile(nextPath, 'utf8');
      await writeFile(nextPath, stripRemoteHeadTags(html), 'utf8');
    }
  }
}

await rm(targetDist, { recursive: true, force: true });
await mkdir(targetDist, { recursive: true });
await cp(sourceDist, targetDist, { recursive: true, force: true });
await patchHtmlFiles(targetDist);
