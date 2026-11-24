# Como escurece

O nível (0 a 90) é o percentual de luz que sai da tela. A luz que sobra é:

```
T = 1 − nível / 100
```

Por isso o nível 90 sempre deixa 10% de luz, com ou sem DDC/CI.

## Duas ferramentas

**Brilho real (DDC/CI).** O próprio monitor reduz a luz de fundo. A imagem continua com o contraste inteiro e não há nada desenhado por cima. O limite é o brilho mínimo do painel, e abaixo da metade muitos monitores ficam com as cores lavadas. O Penumbra usa o DDC/CI só até T = 0,5.

**Camada.** Uma janela preta, transparente aos cliques, cobrindo a tela toda. Funciona em qualquer monitor, mas escurece somando preto à imagem.

## A divisão

A função `split` em `src-tauri/src/levels.rs` decide quanto vai para cada uma:

| Situação | Brilho do monitor | Opacidade da camada |
| --- | --- | --- |
| sem DDC/CI | — | nível / 100 |
| com DDC/CI, nível ≤ 50 | original × T | 0 |
| com DDC/CI, nível > 50 | original × 0,5 | 1 − T / 0,5 |

Exemplo, num monitor com brilho original 90 e nível 70 (T = 0,3): o monitor vai para 45 e a camada fica com 1 − 0,3 / 0,5 = 0,4 de opacidade. A luz final é 0,5 × (1 − 0,4) = 0,3, a mesma que o nível pediu.

## Brilho original

Quando um monitor começa a ser escurecido, o Penumbra lê o brilho atual pelo DDC/CI e guarda como "original". Voltar ao nível 0, desligar a tela no interruptor ou sair do app regrava esse valor, e só depois que a escrita dá certo o original é descartado. Assim, o que você ajustar no botão do monitor com o Penumbra em 0 passa a valer na próxima vez. Se a leitura falhar, o Penumbra não mexe no DDC e usa só a camada.

## Por que debounce

Gravar pelo DDC/CI é lento (dezenas de milissegundos, às vezes mais) e alguns monitores travam com muitas escritas seguidas. Enquanto o slider anda, a camada responde na hora; o brilho real e o arquivo de configuração só são gravados depois de 150 ms sem mudança, por uma thread própria.

Voltar para a [documentação](README.md).
