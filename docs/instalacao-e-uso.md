# Instalação e uso

## Instalar

1. Gere o instalador com `npx tauri build` (ou use um já gerado).
2. Abra `Penumbra_x.y.z_x64-setup.exe` pelo Explorer, com duplo clique.
3. No fim, marque "criar atalho na área de trabalho" se quiser.

> Rode o instalador pelo Explorer, não de dentro de um terminal aberto por um app empacotado (MSIX). Nesses terminais o Windows redireciona o `AppData` para a pasta privada do pacote, e o Explorer não enxerga a instalação: o atalho fica com o ícone em branco.

## Usar

1. Clique no ícone da coruja na bandeja.
2. Em cada tela: o interruptor liga ou desliga o escurecimento, o slider vai de 0 a 90%, e os botões 25, 50 e 75% são atalhos.
3. A fileira **Todas as telas** aplica o mesmo nível em todas (Normal volta tudo a 0).
4. Arraste o cabeçalho ou o fundo para mover a janela; puxe as bordas ou os cantos para redimensionar. A interface cresce junto, e a altura acompanha o número de monitores.
5. A janela fecha sozinha ao perder o foco, ou com Esc.

Embaixo do slider, cada tela mostra o modo em uso:

- **Brilho real + camada**: o monitor aceita DDC/CI.
- **Camada**: só a camada escura.

## Bandeja

O botão direito no ícone traz:

- Escurecer todas (25, 50 ou 75%)
- Restaurar todas
- Iniciar com o Windows
- Sair (devolve o brilho original e remove as camadas)

O tooltip do ícone mostra o nível de cada tela.

## Onde fica a configuração

`%APPDATA%\Penumbra\config.json` guarda o nível e o estado de cada tela, o tema e, enquanto uma tela está escurecida, o brilho original dela.

## Se algo der errado

- **O app foi encerrado à força com uma tela escurecida.** O brilho do monitor fica como estava. Abra o Penumbra de novo e leve o slider a 0, ou use Ctrl+Alt+End duas vezes.
- **Um jogo ficou por cima da camada.** Jogos em tela cheia exclusiva desenham acima de qualquer janela. Em tela cheia sem borda a camada funciona.
- **O monitor só mostra "Camada".** Ele não respondeu ao DDC/CI. Alguns monitores trazem essa opção desligada no menu (OSD).

Voltar para a [documentação](README.md).
