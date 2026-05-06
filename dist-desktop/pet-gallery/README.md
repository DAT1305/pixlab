# Pet Gallery

Put pets here to ship them with the desktop app repo.

Folder format:

```text
pet-gallery/
  manifest.json
  my-pet/
    pet.json
    spritesheet.webp
    thumbnail.png
```

Manifest item format:

```json
{
  "id": "my-pet",
  "displayName": "My Pet",
  "description": "Short description",
  "folder": "my-pet",
  "thumbnail": "thumbnail.png"
}
```

`thumbnail` is optional. The app can render the first idle frame from `spritesheet.webp`.
