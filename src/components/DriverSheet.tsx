import type { DriverState } from '../types';

interface Props {
  driver: DriverState;
  onInstall(): void;
  onClose(): void;
}

/** Bottom sheet that offers the PawnIO driver and follows the install through `driver`. */
export function DriverSheet({ driver, onInstall, onClose }: Props) {
  return (
    // The scrim closes the sheet, except while installing.
    <div class="scrim" onClick={() => driver !== 'installing' && onClose()}>
      <div class="sheet" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
        {driver === 'installing' ? (
          <>
            <div class="sheet-installing">
              <span class="spinner" />
              <span class="sheet-title">Instalando PawnIO…</span>
            </div>
            <span class="sheet-text">Se o Windows pedir permissão, clique em “Sim”. Não feche esta janela.</span>
            <div class="progress">
              <div class="progress-bar" />
            </div>
          </>
        ) : driver === 'failed' ? (
          <>
            <span class="sheet-title err">Não foi possível instalar o driver</span>
            <span class="sheet-text">
              A permissão de administrador foi negada ou a instalação foi bloqueada pelo antivírus. As outras métricas continuam funcionando.
            </span>
            <div class="sheet-actions">
              <button class="btn-secondary" onClick={onClose}>
                Agora não
              </button>
              <button class="btn-primary" onClick={onInstall}>
                Tentar de novo
              </button>
            </div>
          </>
        ) : (
          <>
            <span class="sheet-title">Temperatura da CPU precisa de um driver</span>
            <span class="sheet-text">
              Para ler o sensor da CPU, o app usa o PawnIO — um driver gratuito e assinado digitalmente. A instalação leva poucos segundos e o Windows vai pedir permissão de administrador.
            </span>
            <div class="sheet-actions">
              <button class="btn-secondary" onClick={onClose}>
                Agora não
              </button>
              <button class="btn-primary" onClick={onInstall}>
                Instalar
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
