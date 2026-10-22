# Módulos do PawnIO

Módulos assinados do [PawnIO](https://pawnio.eu/), embutidos no executável com `include_bytes!`
(`crates/engine/src/pawnio/mod.rs`). Só leem temperatura da CPU; o driver em si não vem junto,
é instalado pelo usuário via `winget install --id namazso.PawnIO`.

| Arquivo | Uso | SHA-256 |
|---|---|---|
| `AMDFamily17.bin` | AMD famílias 17h a 1Ah (Zen 1 a Zen 5): `ioctl_read_smn`, registrador `0x00059800` (Tctl) | `dae74615761b78bdf064dfb3e136252ddcc6fc727d88f14738d0e5800d427a91` |
| `IntelMSR.bin` | Intel: `ioctl_read_msr`, MSR `0x1A2` (TjMax) e `0x1B1` (temperatura do pacote) | `d6ed85d65ab17a22f813ef98207d6d537155ee2ded5976a21cb48413c9b92e5f` |

- **Origem:** release `0.2.11` de [namazso/PawnIO.Modules](https://github.com/namazso/PawnIO.Modules),
  publicada em 2026-08-30. Arquivo baixado:
  <https://github.com/namazso/PawnIO.Modules/releases/download/0.2.11/release_0_2_11.zip>
  (SHA-256 `43608cb89bc84247fef1368a139013f7d043e17db6d6c8dfc9b46bf0905a81f4`). Os dois `.bin` foram
  copiados desse zip sem alteração.
- **Código-fonte:** `AMDFamily17.p` e `IntelMSR.p` na tag
  [`0.2.11`](https://github.com/namazso/PawnIO.Modules/tree/0.2.11).
- **Licença:** GNU LGPL 2.1 ou posterior (`SPDX-License-Identifier: LGPL-2.1-or-later`), texto em
  <https://github.com/namazso/PawnIO.Modules/blob/0.2.11/COPYING>.

Pra atualizar: baixe a release nova numa pasta vazia, copie só esses dois arquivos e atualize esta
tabela (versão, URL e hashes).
