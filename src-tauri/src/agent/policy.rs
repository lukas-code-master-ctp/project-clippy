//! Capa de políticas: ninguna acción sensible se ejecuta sin que el usuario la confirme.
//!
//! La regla vive en los tipos, no en el prompt: una herramienta sensible solo puede
//! ejecutarse recibiendo una `Approval`, y una `Approval` solo se crea en este módulo,
//! cuando el usuario aprueba una tarjeta. Se consume al usarla: un "sí" vale para una acción.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::oneshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Lo usan las pruebas; el historial (hito 4) lo va a registrar.
pub enum Sensitivity {
    /// Solo lee o informa: se ejecuta sin preguntar.
    Read,
    /// Cambia algo fuera de Brote (enviar, crear, borrar, abrir): exige confirmación.
    Sensitive,
}

/// Prueba de que el usuario aprobó una acción concreta. No se puede clonar ni crear fuera de aquí.
#[derive(Debug)]
pub struct Approval {
    _private: (),
}

/// Confirmaciones pendientes, esperando la respuesta del usuario.
#[derive(Default)]
pub struct Gate {
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, oneshot::Sender<bool>>>,
}

impl Gate {
    /// Abre una confirmación. Devuelve su id (para la tarjeta) y por dónde llegará la respuesta.
    pub fn open(&self) -> (u64, oneshot::Receiver<bool>) {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        (id, rx)
    }

    /// La respuesta del usuario. Devuelve `false` si la confirmación ya no existe.
    pub fn resolve(&self, id: u64, approved: bool) -> bool {
        match self.pending.lock().unwrap().remove(&id) {
            Some(tx) => tx.send(approved).is_ok(),
            None => false,
        }
    }

    /// Descarta todas las pendientes (por ejemplo, al detener a Brote). Cuentan como "no".
    pub fn cancel_all(&self) {
        self.pending.lock().unwrap().clear();
    }
}

/// Espera la respuesta. Solo un "sí" explícito y a tiempo produce una `Approval`.
pub async fn wait(rx: oneshot::Receiver<bool>, timeout: Duration) -> Option<Approval> {
    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(true)) => Some(Approval { _private: () }),
        _ => None,
    }
}

#[cfg(test)]
pub fn approval_for_tests() -> Approval {
    Approval { _private: () }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn solo_un_si_explicito_aprueba() {
        let gate = Gate::default();
        let (id, rx) = gate.open();
        assert!(gate.resolve(id, true));
        assert!(wait(rx, Duration::from_secs(1)).await.is_some());

        let (id, rx) = gate.open();
        gate.resolve(id, false);
        assert!(wait(rx, Duration::from_secs(1)).await.is_none());
    }

    #[tokio::test]
    async fn sin_respuesta_o_cancelada_es_un_no() {
        let gate = Gate::default();
        let (_id, rx) = gate.open();
        assert!(wait(rx, Duration::from_millis(20)).await.is_none());

        let (id, rx) = gate.open();
        gate.cancel_all();
        assert!(wait(rx, Duration::from_secs(1)).await.is_none());
        assert!(!gate.resolve(id, true), "una confirmación cancelada no se puede aprobar después");
    }

    #[tokio::test]
    async fn una_confirmacion_se_responde_una_sola_vez() {
        let gate = Gate::default();
        let (id, rx) = gate.open();
        assert!(gate.resolve(id, true));
        assert!(!gate.resolve(id, true));
        assert!(wait(rx, Duration::from_secs(1)).await.is_some());
    }
}
