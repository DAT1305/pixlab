import { cp, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const desktopRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(desktopRoot, '..');
const sourceDist = path.join(repoRoot, 'dist');
const targetDist = path.join(desktopRoot, 'dist-desktop');
const desktopPetGallery = path.join(desktopRoot, 'pet-gallery');
const targetPetGallery = path.join(targetDist, 'pet-gallery');

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

async function copyDesktopPetGallery() {
  if (!existsSync(desktopPetGallery)) return;
  await rm(targetPetGallery, { recursive: true, force: true });
  await cp(desktopPetGallery, targetPetGallery, { recursive: true, force: true });
  await writeGeneratedPetGalleryManifest(targetPetGallery);
}

async function writeGeneratedPetGalleryManifest(galleryRoot) {
  const entries = await readdir(galleryRoot, { withFileTypes: true });
  const pets = [];
  for (const entry of entries) {
    if (!entry.isDirectory()) continue;
    const folder = entry.name;
    const petJsonPath = path.join(galleryRoot, folder, 'pet.json');
    const spritesheetPath = path.join(galleryRoot, folder, 'spritesheet.webp');
    if (!existsSync(petJsonPath) || !existsSync(spritesheetPath)) continue;
    try {
      const pet = JSON.parse(await readFile(petJsonPath, 'utf8'));
      const displayName = String(pet.displayName || pet.name || pet.id || folder).trim();
      pets.push({
        id: String(pet.id || folder).trim(),
        displayName,
        description: String(pet.description || displayName).trim(),
        folder,
      });
    } catch (error) {
      console.warn(`Skipping invalid pet gallery item ${folder}:`, error);
    }
  }
  pets.sort((a, b) => a.displayName.localeCompare(b.displayName));
  await writeFile(
    path.join(galleryRoot, 'manifest.json'),
    `${JSON.stringify({ version: 1, pets }, null, 2)}\n`,
    'utf8',
  );
}

await rm(targetDist, { recursive: true, force: true });
await mkdir(targetDist, { recursive: true });
await cp(sourceDist, targetDist, { recursive: true, force: true });
await copyDesktopPetGallery();
await patchHtmlFiles(targetDist);
