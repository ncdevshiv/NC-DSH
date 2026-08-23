/// A typed task payload targeted at a document/realm pair.
///
/// This is the shared envelope only. The owner adapter still decides how to
/// validate currentness, enter the realm, and dispatch the payload.
#[derive(Debug, Clone)]
pub(crate) struct DocumentRealmTask<Owner, Realm, Payload> {
    owner: Owner,
    realm_id: Realm,
    payload: Payload,
}

impl<Owner, Realm, Payload> DocumentRealmTask<Owner, Realm, Payload> {
    pub(crate) fn new(owner: Owner, realm_id: Realm, payload: Payload) -> Self {
        Self {
            owner,
            realm_id,
            payload,
        }
    }

    pub(crate) fn owner(&self) -> Owner
    where
        Owner: Copy,
    {
        self.owner
    }

    pub(crate) fn realm_id(&self) -> Realm
    where
        Realm: Copy,
    {
        self.realm_id
    }

    pub(crate) fn payload(&self) -> &Payload {
        &self.payload
    }

    pub(crate) fn into_payload(self) -> Payload {
        self.payload
    }

    pub(crate) fn into_parts(self) -> (Owner, Realm, Payload) {
        (self.owner, self.realm_id, self.payload)
    }
}

impl<Owner, Realm, Payload> PartialEq for DocumentRealmTask<Owner, Realm, Payload>
where
    Owner: PartialEq,
    Realm: PartialEq,
    Payload: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.owner == other.owner
            && self.realm_id == other.realm_id
            && self.payload == other.payload
    }
}

impl<Owner, Realm, Payload> Eq for DocumentRealmTask<Owner, Realm, Payload>
where
    Owner: Eq,
    Realm: Eq,
    Payload: Eq,
{
}
