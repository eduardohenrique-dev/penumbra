# Arquitetura

O Penumbra é um app Tauri 2: o núcleo em Rust fala direto com o Win32, e a interface é HTML, CSS e JavaScript puro, sem framework e sem build.

## Processo e janelas

```
bandeja (sempre)
  └── núcleo Rust (core.rs)
        ├── uma camada Win32 por monitor (overlay.rs, thread própria)
        ├── DDC/CI por monitor (ddc.rs)
        └── config.json (state.rs)

janela da interface (só enquanto aberta)
  └── WebView2 → ui/index.html → comandos Tauri
```

- **Janela sob demanda.** A janela da interface é criada ao clicar na bandeja e destruída ao fechar. Sem ela, o WebView2 não fica carregado, e o app pesa cerca de 16 MB.
- **Instância única.** Abrir o Penumbra de novo só mostra a janela da cópia que já está rodando (`tauri-plugin-single-instance`).
- **Camadas fora do WebView.** Cada camada é uma janela `WS_POPUP` com `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST`: transparente aos cliques, sem foco, fora da barra de tarefas e por cima de tudo. A opacidade vem de `SetLayeredWindowAttributes`. Todas vivem numa thread com laço de mensagens próprio, e o núcleo manda comandos por canal (aplicar, remover, reafirmar o topo).

## Módulos

| Arquivo | Papel |
| --- | --- |
| `levels.rs` | matemática pura do nível; ver [como escurece](como-escurece.md) |
| `state.rs` | lê e grava `%APPDATA%\Penumbra\config.json` |
| `monitors.rs` | enumera os monitores e dá a cada um um id estável |
| `overlay.rs` | cria, ajusta e remove as camadas |
| `ddc.rs` | lê e grava o brilho pelo `dxva2` (DDC/CI) |
| `core.rs` | estado em memória, aplica níveis, debounce do DDC e da gravação |
| `lib.rs` | bandeja, menu, atalhos globais, autostart, comandos da interface |

## Comandos da interface

A interface chama o núcleo por `invoke`: `list_monitors`, `set_level`, `set_enabled`, `set_all`, `fit_window`, `hide_main` e `ui_ready`. Fora do app (aberta direto no navegador), `ui/app.js` troca esses comandos por um simulador com dois monitores de exemplo, o que permite trabalhar no visual sem compilar o Rust.

## Mudanças de tela

Uma thread de vigia confere os monitores a cada 2 segundos. Se algum foi conectado, desconectado ou mudou de posição, o núcleo cria ou remove camadas, reaplica os níveis guardados para cada id e avisa a interface (`monitors-changed`). Na mesma volta, ele reafirma as camadas no topo, porque outras janelas "sempre por cima" podem passar à frente delas.

Voltar para a [documentação](README.md).
