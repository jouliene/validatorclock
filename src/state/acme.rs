use super::AppState;

impl AppState {
    pub(crate) async fn acme_challenge(&self, token: &str) -> Option<String> {
        self.acme_challenges.read().await.get(token).cloned()
    }

    pub(crate) async fn insert_acme_challenge(&self, token: String, key_authorization: String) {
        self.acme_challenges
            .write()
            .await
            .insert(token, key_authorization);
    }

    pub(crate) async fn remove_acme_challenge(&self, token: &str) {
        self.acme_challenges.write().await.remove(token);
    }
}
