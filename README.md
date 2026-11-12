# Orbiz

Un petit orbe flottant pour Windows, toujours à portée de souris.

- **Un clic** : un anneau de raccourcis (apps, programmes, fenêtres d'Orbiz), configurable dans `%APPDATA%\orbiz\actions.json`.
- **Chat IA** (clé API OpenAI) qui peut piloter un navigateur et les apps compatibles [iris-link](https://github.com/Iris-universe/iris-briques) ; dépenses estimées et limite de coût par message.
- **Notes** et **mini navigateur** dans des fenêtres flottantes, avec des raccourcis globaux (Ctrl+Maj+Espace, Ctrl+Maj+N, Ctrl+Maj+B).

Rust + [Tauri 2](https://tauri.app), Windows uniquement. Compiler : `cargo build --release` (les briques partagées sont attendues dans `../iris-briques`).
