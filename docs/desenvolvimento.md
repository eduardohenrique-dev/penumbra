# Desenvolvimento

## Requisitos

- Windows 10 ou 11
- Node 22 ou mais novo
- Rust estável (toolchain MSVC)
- Visual Studio Build Tools com a carga de trabalho de C++

## Comandos

```bash
npm install
npx tauri dev     # modo desenvolvimento, com console
npx tauri build   # instalador NSIS em src-tauri/target/release/bundle/nsis
```

Para mexer só no visual, abra `ui/index.html` direto no navegador: a interface usa monitores de exemplo quando não está dentro do app.

## Testes

```bash
cd src-tauri
cargo test --lib
```

### Testes manuais

Estes mexem nas telas de verdade por alguns segundos e por isso ficam marcados com `#[ignore]`:

```bash
cargo test manual_overlay -- --ignored --nocapture   # camada em cada monitor
cargo test manual_ddc     -- --ignored --nocapture   # lê e regrava o brilho DDC/CI
cargo test manual_core    -- --ignored --nocapture   # cenário completo a 70%, com restauração
```

Antes de interromper um teste manual, deixe-o terminar: encerrar no meio pode deixar o brilho do monitor alterado. Para voltar, abra o app e leve o slider a 0.

## Ícones

O logo é `branding/logo.svg`. Para regenerar todos os ícones:

```bash
node branding/render.mjs
npx tauri icon app-icon.png
node branding/make-ico.mjs
```

O último passo grava um `.ico` com bitmaps. O `.ico` gerado pelo `tauri icon` usa PNG por dentro, e o Explorer não o mostra em atalhos.

Voltar para a [documentação](README.md).
