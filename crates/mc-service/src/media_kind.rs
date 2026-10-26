//! The media-kind vocabulary MC shares across its media modules (story 3), and
//! the ONE home of every per-kind fact MC holds about a participant
//! (ADR-0036 §5 amendment 2026-10-25: every per-kind fact is keyed by
//! `MediaKind`, never a hand-named audio/video pair).
//!
//! A LEAF module on purpose (Gate 1, code-quality CQ1): `media_signaling`
//! depends on `media_routing`, never the reverse, and both need the kind and
//! the publish set. Homing them here keeps that dependency one-way;
//! `media_signaling` re-exports [`MediaKindLabel`] so its existing call sites
//! are unchanged.
//!
//! The wire carries every per-kind fact as a repeated entry keyed by
//! `MediaKind` (docs/protocol/CONVENTIONS.md §6). This module owns the ONE
//! parser for those lists ([`parse_kind_list`]) and the per-kind state types
//! the meeting actor stores: [`ClientMediaStates`] (publish set + client mute
//! in one value per kind) and [`ServerMuteAttribution`] (server mute + who
//! imposed it in one value per kind). Per-kind values are `[T; 2]` laid out by
//! [`MediaKindLabel::index`] and read only through its accessors, so no caller
//! outside this module destructures an `[audio, video]` pair.
//!
//! No key material. The only identity is [`ServerMuteAttribution`]'s
//! MC-produced requester ids.

use proto_gen::dark_tower::signaling::v1::{ClientMediaState, MediaKind};

/// A media kind MC serves, and the label of a series whose subject has one
/// (`media_kind` label; story 3 OBS-A).
///
/// ANCHOR (DRY): the `media_kind` row of `docs/observability/label-taxonomy.md`
/// is the single source of truth for this value set; the test
/// `media_kind_label_matches_the_taxonomy_row` reads that row, so the two cannot
/// drift silently. The key constant is `common::observability::labels::MEDIA_KIND_LABEL`.
///
/// Distinct from the wire `proto_gen::dark_tower::signaling::v1::MediaKind`,
/// which has four values (`UNSPECIFIED`, `AUDIO`, `VIDEO_CAMERA`,
/// `VIDEO_SCREEN`). This type has one variant per kind MC SERVES today (audio,
/// camera); screen share has no per-kind state here because nobody publishes
/// it, and the wire encoder emits it as a constant (`webtransport::handler`).
///
/// # Two wire mappings, deliberately NOT collapsed
///
/// - [`Self::from_proto`] is the LABEL projection: every labellable wire kind
///   maps to a label (screen share is `video` until session 2's
///   `content_kind`), so a recorder is never handed an unlabelled kind.
/// - [`served_kind`] is ADMISSION: the closed set of kinds this story SERVES.
///   It refuses `VIDEO_SCREEN` ([`KindRefusal::Unsupported`]), so merging the
///   two would fail open on screen share. The
///   `served_kind_agrees_with_the_label_projection` test pins that wherever
///   admission yields a kind, the label projection yields the same one.
///
/// Since story 3 MC-A this is ALSO the slot table's kind (per-kind slot
/// vectors, edges, egress ids), so there is no third kind enum.
///
/// MC and the client SDK carry this label; MH never does (ADR-0036 §7: MH is
/// type-blind).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKindLabel {
    /// `audio` (the wire `MEDIA_KIND_AUDIO`).
    Audio,
    /// `video`. As a served kind: the camera only ([`served_kind`] refuses
    /// screen share). As a label projection ([`Self::from_proto`]) a screen
    /// kind also labels `video`.
    Video,
}

impl MediaKindLabel {
    /// Every variant, for exhaustive metric-vocabulary tests and zero-init.
    pub const ALL: [Self; 2] = [Self::Audio, Self::Video];

    /// Bounded metric-label form.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Video => "video",
        }
    }

    /// This kind's position in [`Self::ALL`], for per-kind `[T; 2]` tallies
    /// (read back with [`Self::of`]). Exhaustive, so the positional layout is
    /// defined HERE and nowhere else.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Audio => 0,
            Self::Video => 1,
        }
    }

    /// This kind's entry of a per-kind `[T; 2]` laid out by [`Self::index`]
    /// (as built by [`Self::per_kind`]). Infallible: destructures, never
    /// indexes.
    #[must_use]
    pub fn of<T>(self, per_kind: [T; 2]) -> T {
        let [audio, video] = per_kind;
        match self {
            Self::Audio => audio,
            Self::Video => video,
        }
    }

    /// This kind's entry of a per-kind `[T; 2]`, by reference. Infallible:
    /// destructures, never indexes (`clippy::indexing_slicing`).
    #[must_use]
    pub fn of_ref<T>(self, per_kind: &[T; 2]) -> &T {
        let [audio, video] = per_kind;
        match self {
            Self::Audio => audio,
            Self::Video => video,
        }
    }

    /// This kind's entry of a per-kind `[T; 2]`, mutably. Infallible.
    pub fn of_mut<T>(self, per_kind: &mut [T; 2]) -> &mut T {
        let [audio, video] = per_kind;
        match self {
            Self::Audio => audio,
            Self::Video => video,
        }
    }

    /// The other kind.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Audio => Self::Video,
            Self::Video => Self::Audio,
        }
    }

    /// The wire kind MC serves for this kind (camera for video).
    #[must_use]
    pub const fn to_proto(self) -> MediaKind {
        match self {
            Self::Audio => MediaKind::Audio,
            Self::Video => MediaKind::VideoCamera,
        }
    }

    /// A per-kind `[T; 2]` built from `f`, laid out by [`Self::index`].
    #[must_use]
    pub fn per_kind<T>(mut f: impl FnMut(Self) -> T) -> [T; 2] {
        Self::ALL.map(&mut f)
    }

    /// The label for a wire media kind, or `None` for `UNSPECIFIED`.
    ///
    /// Exhaustive with no wildcard arm, so a new wire variant is a compile error
    /// here rather than a silent mislabel.
    #[must_use]
    pub const fn from_proto(kind: MediaKind) -> Option<Self> {
        match kind {
            MediaKind::Audio => Some(Self::Audio),
            MediaKind::VideoCamera => Some(Self::Video),
            // DELIBERATE: screen share is `media_kind="video"` plus
            // `content_kind` in the taxonomy. `content_kind` is declared but not
            // carried (session 2, `mediakind-metric-labels`), so a screen slot
            // is labelled `video` until it is.
            MediaKind::VideoScreen => Some(Self::Video),
            // Rejected at every parse boundary that admits a kind; it has no
            // honest label.
            MediaKind::Unspecified => None,
        }
    }
}

/// Every KNOWN non-`UNSPECIFIED` wire kind, in wire-number order: what every
/// server snapshot list carries exactly one entry for (CONVENTIONS §6). The
/// `known_wire_kinds_are_every_value_the_enum_accepts` test sweeps
/// `MediaKind::try_from` over a range, so a new wire variant left out of this
/// list fails the build's tests rather than silently missing from snapshots.
pub const KNOWN_WIRE_KINDS: [MediaKind; 3] = [
    MediaKind::Audio,
    MediaKind::VideoCamera,
    MediaKind::VideoScreen,
];

/// Why a wire kind is not a kind MC serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindRefusal {
    /// A real kind this story does not serve (`VIDEO_SCREEN`).
    Unsupported,
    /// `MEDIA_KIND_UNSPECIFIED`, or an i32 outside the enum (the enum is open
    /// on the wire, so an unknown number decodes as an integer).
    Unspecified,
}

/// ADMISSION: the closed set of kinds MC serves, from a raw wire value. THE
/// one per-entry wire-to-kind decode: [`parse_kind_list`] and the capability
/// parser both call it.
///
/// Exhaustive with no wildcard arm over the decoded enum, so a new wire
/// variant is a compile error here. See [`MediaKindLabel`] for why this and
/// [`MediaKindLabel::from_proto`] are two mappings.
///
/// # Errors
///
/// [`KindRefusal`] for every value outside {`AUDIO`, `VIDEO_CAMERA`}.
pub fn served_kind(raw: i32) -> Result<MediaKindLabel, KindRefusal> {
    match MediaKind::try_from(raw) {
        Ok(MediaKind::Audio) => Ok(MediaKindLabel::Audio),
        Ok(MediaKind::VideoCamera) => Ok(MediaKindLabel::Video),
        Ok(MediaKind::VideoScreen) => Err(KindRefusal::Unsupported),
        Ok(MediaKind::Unspecified) | Err(_) => Err(KindRefusal::Unspecified),
    }
}

/// Why a kind-keyed list (CONVENTIONS §6) is refused WHOLE.
///
/// Wraps the per-entry [`KindRefusal`] rather than restating it, so there is
/// one per-entry decode ([`served_kind`]). Each CALLER maps this onto its own
/// counted vocabulary, which is where the two screen classifications stay
/// visible: the mute surfaces count `Kind(Unsupported)` as `not_published`
/// (nobody publishes screen), the join as `publish_kind_invalid`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindListRefusal {
    /// An entry named a kind MC does not serve, or no valid kind at all.
    Kind(KindRefusal),
    /// Two entries named the same kind, whatever their values.
    Duplicate,
}

/// Parse a kind-keyed repeated field (CONVENTIONS §6) into one value per
/// served kind, laid out by [`MediaKindLabel::index`]: `None` = the list has no
/// entry for that kind.
///
/// THE ONE kind-list parser, generic over the entry: `split` yields an entry's
/// raw `media_kind` and its value (`PublishedKind` -> `client_muted`,
/// `MuteChange` -> `muted`, a bare `MediaKind` -> `()`).
///
/// Fails on the FIRST unserved entry or duplicate, so a long hostile list costs
/// at most three iterations (a third served entry is necessarily a duplicate)
/// and allocates nothing. Rejected, never ignored or deduplicated.
///
/// # Errors
///
/// [`KindListRefusal`]; an EMPTY list is not an error here (it is a legal
/// receive-only declaration) -- request callers refuse it themselves.
pub fn parse_kind_list<I, T>(
    items: impl IntoIterator<Item = I>,
    split: impl Fn(I) -> (i32, T),
) -> Result<[Option<T>; 2], KindListRefusal> {
    let mut out: [Option<T>; 2] = [None, None];
    for item in items {
        let (raw, value) = split(item);
        let kind = served_kind(raw).map_err(KindListRefusal::Kind)?;
        let slot = kind.of_mut(&mut out);
        if slot.is_some() {
            return Err(KindListRefusal::Duplicate);
        }
        *slot = Some(value);
    }
    Ok(out)
}

/// What a participant publishes, fixed for its membership (story 3 R-2).
///
/// A DERIVED VIEW: in production it comes only from
/// [`ClientMediaStates::publish_set`], so the publish set and the client-mute
/// state cannot drift. [`Self::of`] exists for tests and fixtures. Equality is
/// SET equality: order on the wire is irrelevant, which is what the reconnect
/// comparison needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PublishSet([bool; 2]);

/// A join `published_kinds` list MC refuses (join token `publish_kind_invalid`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishKindInvalid;

impl PublishSet {
    /// Receive-only: publishes nothing.
    pub const NONE: Self = Self([false; 2]);

    /// A set publishing exactly `kinds` (duplicates collapse). For tests and
    /// fixtures; production builds it from [`ClientMediaStates::publish_set`].
    #[must_use]
    pub fn of(kinds: &[MediaKindLabel]) -> Self {
        let mut set = [false; 2];
        for kind in kinds {
            *kind.of_mut(&mut set) = true;
        }
        Self(set)
    }

    /// Does this participant publish `kind`?
    #[must_use]
    pub fn publishes(self, kind: MediaKindLabel) -> bool {
        *kind.of_ref(&self.0)
    }

    /// Does this participant publish nothing?
    #[must_use]
    pub fn is_empty(self) -> bool {
        self == Self::NONE
    }

    /// The published kinds, in [`MediaKindLabel::ALL`] order (audio first).
    pub fn kinds(self) -> impl Iterator<Item = MediaKindLabel> {
        MediaKindLabel::ALL
            .into_iter()
            .filter(move |k| self.publishes(*k))
    }

    /// Does this set contain EVERY kind of `kinds`? THE one "every named kind
    /// is published" predicate (code-quality CR-3, DRY F7): client mute,
    /// server mute and unmute all decide `not_published` through it.
    #[must_use]
    pub fn covers(self, mut kinds: impl Iterator<Item = MediaKindLabel>) -> bool {
        kinds.all(|k| self.publishes(k))
    }
}

/// One served kind's CLIENT state, as MC knows it (the wire
/// `ClientMediaState`). There is no unknown state: MC knows every kind from the
/// join's `published_kinds` (ADR-0036 §5 amendment 2026-10-25).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClientKindState {
    /// The participant does not publish this kind (fixed for the membership).
    #[default]
    NotPublished,
    /// Published and not client-muted.
    Live,
    /// Published and client-muted (enforced at capture by the client).
    ClientMuted,
}

impl ClientKindState {
    /// A PUBLISHED kind's state from its client-mute flag.
    #[must_use]
    pub const fn published(client_muted: bool) -> Self {
        if client_muted {
            Self::ClientMuted
        } else {
            Self::Live
        }
    }

    /// The inverse of [`Self::published`]: `Some(client_muted)` for a
    /// published kind, `None` for `NotPublished`.
    #[must_use]
    pub const fn client_muted(self) -> Option<bool> {
        match self {
            Self::NotPublished => None,
            Self::Live => Some(false),
            Self::ClientMuted => Some(true),
        }
    }

    /// THE one MC -> wire mapping (MC never decodes `ClientMediaState`).
    /// Exhaustive, and never yields `UNSPECIFIED`.
    #[must_use]
    pub const fn to_proto(self) -> ClientMediaState {
        match self {
            Self::NotPublished => ClientMediaState::NotPublished,
            Self::Live => ClientMediaState::Live,
            Self::ClientMuted => ClientMediaState::ClientMuted,
        }
    }
}

/// A participant's CLIENT state for every served kind: its publish set and its
/// client mute in ONE value per kind (story 3 R-2, R-15, R-17), so the two
/// cannot disagree. Built at admission from the join's `published_kinds` and written
/// afterwards only by [`Self::apply`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClientMediaStates([ClientKindState; 2]);

impl ClientMediaStates {
    /// Publishes nothing (a receive-only participant).
    pub const NOT_PUBLISHING: Self = Self([ClientKindState::NotPublished; 2]);

    /// From a parsed declaration: `Some(client_muted)` for a published kind,
    /// `None` for a kind the participant does not publish.
    #[must_use]
    pub fn from_declared(declared: [Option<bool>; 2]) -> Self {
        Self(declared.map(|d| d.map_or(ClientKindState::NotPublished, ClientKindState::published)))
    }

    /// Every kind of `publishes` LIVE (unmuted), the rest NOT_PUBLISHED. For
    /// tests and fixtures; production parses the join declaration.
    #[must_use]
    pub fn live(publishes: PublishSet) -> Self {
        Self(publishes.0.map(|p| {
            if p {
                ClientKindState::Live
            } else {
                ClientKindState::NotPublished
            }
        }))
    }

    /// Parse the join's `published_kinds`.
    ///
    /// # Errors
    ///
    /// [`PublishKindInvalid`] for an entry whose kind is `UNSPECIFIED`, an
    /// unknown number or `VIDEO_SCREEN`, or a duplicate kind -- rejected, never
    /// ignored or deduplicated ([`parse_kind_list`]).
    pub fn parse_declaration(
        kinds: &[proto_gen::dark_tower::signaling::v1::PublishedKind],
    ) -> Result<Self, PublishKindInvalid> {
        parse_kind_list(kinds, |k| (k.media_kind, k.client_muted))
            .map(Self::from_declared)
            .map_err(|_| PublishKindInvalid)
    }

    /// `kind`'s state.
    #[must_use]
    pub fn get(self, kind: MediaKindLabel) -> ClientKindState {
        kind.of(self.0)
    }

    /// The publish set this state implies (THE production constructor of a
    /// [`PublishSet`]).
    #[must_use]
    pub fn publish_set(self) -> PublishSet {
        PublishSet(self.0.map(|s| s != ClientKindState::NotPublished))
    }

    /// The media-policy projection: which kinds are client-muted.
    #[must_use]
    pub fn effective(self) -> ClientMutedKinds {
        ClientMutedKinds(self.0.map(|s| s == ClientKindState::ClientMuted))
    }

    /// Merge one validated client-mute change into this state.
    ///
    /// Takes ONLY a [`PublishedMuteChange`] (proven at the connection), and
    /// re-checks it against THIS state's publish set, so a mismatched proof is
    /// never silently counted as `unchanged` (code-quality CR-4): it is the
    /// counted `not_published` refusal instead.
    ///
    /// # Errors
    ///
    /// [`NotPublished`] when the change names a kind this state does not
    /// publish; nothing is applied.
    pub fn apply(
        self,
        change: PublishedMuteChange,
    ) -> Result<KindDecision<ClientKindState>, NotPublished> {
        if !change.0.names_only(self.publish_set()) {
            return Err(NotPublished);
        }
        let state = MediaKindLabel::per_kind(|k| {
            change
                .0
                .get(k)
                .map_or(self.get(k), ClientKindState::published)
        });
        Ok(KindDecision::between(self.0, state))
    }

    /// This state as a change naming every PUBLISHED kind with its client
    /// mute (`None` for a receive-only state). A declaration's own kinds ARE
    /// its publish set, so the proof holds by construction: the reconnect
    /// adoption has no fallible step to swallow (code-quality CR-5).
    #[must_use]
    pub fn as_change(self) -> Option<PublishedMuteChange> {
        MuteChange::from_asks(self.0.map(ClientKindState::client_muted)).map(PublishedMuteChange)
    }
}

impl From<KindDecision<ClientKindState>> for ClientMediaStates {
    fn from(decision: KindDecision<ClientKindState>) -> Self {
        Self(decision.state)
    }
}

/// A source's SERVER-mute flags, per kind (story 3 R-16). One of the two
/// halves of `media_routing::SourceMute`, and a DISTINCT TYPE from
/// [`ClientMutedKinds`] on purpose: `media_signaling::directive::build_send_directive`
/// takes THIS type, so handing it the client-mute flags is a type error, not a
/// review comment (ADR-0036 §5: the directive never depends on client mute).
/// Converting one into the other requires an explicit, visible construction --
/// there is no `From` between them, and there must never be one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ServerMutedKinds([bool; 2]);

impl ServerMutedKinds {
    /// Server-muted on neither kind.
    pub const NONE: Self = Self([false; 2]);

    /// Server-muted on every kind: the fail-CLOSED default for a source
    /// missing from a mute projection.
    pub const ALL_MUTED: Self = Self([true; 2]);

    /// Test fixture: from per-kind server-mute flags laid out by
    /// [`MediaKindLabel::index`]. Production builds this ONLY through
    /// [`ServerMuteAttribution::muted_kinds`] (code-quality CR-1).
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn from_server_flags(flags: [bool; 2]) -> Self {
        Self(flags)
    }

    /// Is `kind` server-muted?
    #[must_use]
    pub fn is_muted(self, kind: MediaKindLabel) -> bool {
        kind.of(self.0)
    }

    /// Is any kind server-muted?
    #[must_use]
    pub fn any(self) -> bool {
        self.0.contains(&true)
    }
}

/// A source's CLIENT-mute flags, per kind (story 3 R-17). See
/// [`ServerMutedKinds`] for why the two are distinct types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClientMutedKinds([bool; 2]);

impl ClientMutedKinds {
    /// Client-muted on neither kind.
    pub const NONE: Self = Self([false; 2]);

    /// Test fixture: from per-kind client-mute flags, laid out by
    /// [`MediaKindLabel::index`]. Production builds this ONLY through
    /// [`ClientMediaStates::effective`] (code-quality CR-1).
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn from_client_flags(flags: [bool; 2]) -> Self {
        Self(flags)
    }

    /// Is `kind` client-muted?
    #[must_use]
    pub fn is_muted(self, kind: MediaKindLabel) -> bool {
        kind.of(self.0)
    }
}

/// A participant's SERVER mute, per kind, and WHO imposed it: THE single
/// store (code-quality CQ-2). A kind is server-muted iff it has an imposer,
/// so "muted with no attribution" and "attributed but not muted" are
/// unrepresentable; [`Self::muted_kinds`] derives the flags.
///
/// Invariant: an imposer is set when that kind goes unmuted -> muted, cleared
/// when it is unmuted, and an untouched kind keeps its own, so an unmute of one
/// kind never re-attributes the other kind's remaining mute (security S4).
///
/// MC PRODUCES these values (the requester's id from its authenticated
/// connection); they are never parsed off the wire, so nothing validates or
/// truncates them on the way out. Truncation-before-logging is the receiver's
/// obligation (`MediaKindState.server_muted_by`).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ServerMuteAttribution([Option<String>; 2]);

impl ServerMuteAttribution {
    /// Server-muted on neither kind.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// The derived per-kind server-mute flags.
    #[must_use]
    pub fn muted_kinds(&self) -> ServerMutedKinds {
        ServerMutedKinds(MediaKindLabel::per_kind(|k| self.imposer(k).is_some()))
    }

    /// Who imposed `kind`'s mute in force; `None` iff `kind` is not
    /// server-muted.
    #[must_use]
    pub fn imposer(&self, kind: MediaKindLabel) -> Option<&str> {
        kind.of_ref(&self.0).as_deref()
    }

    /// Apply one server-mute request made by `requester`, in ONE step against
    /// THIS store (code-quality CR-2): a named kind takes its asked-for value,
    /// an unnamed kind keeps its own; a kind that moved to muted takes
    /// `requester` as its imposer, a kind that moved to unmuted loses its
    /// imposer, an unmoved kind is untouched. Returns the per-kind decision
    /// (for counting and the whole-request no-op).
    pub fn apply(&mut self, change: MuteChange, requester: &str) -> KindDecision {
        let current = self.muted_kinds().0;
        let decision = KindDecision::between(
            current,
            MediaKindLabel::per_kind(|k| change.get(k).unwrap_or(k.of(current))),
        );
        for kind in MediaKindLabel::ALL {
            if decision.changed(kind) {
                *kind.of_mut(&mut self.0) = decision.state(kind).then(|| requester.to_string());
            }
        }
        decision
    }
}

/// A participant's FULL per-kind mute state: its client state and its server
/// mute with attribution -- the two INDEPENDENT halves that compose (R-15).
/// What the roster entry (`Participant.media_states`) and every
/// `ParticipantMuteUpdate` carry, built from the ONE meeting-actor record.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MediaStates {
    /// Publish set + client mute.
    pub client: ClientMediaStates,
    /// Server mute + who imposed it.
    pub server: ServerMuteAttribution,
}

/// One mute request's per-kind ask (story 3 R-15, R-17): for each kind, `None`
/// (NO ENTRY on the wire: that kind is UNCHANGED) or `Some(muted)`.
///
/// THE ONE PARSER for both `MuteRequest` (client mute) and `ServerMuteRequest`
/// (server mute): both connection paths build one with [`Self::parse`]. A
/// request naming NO kind is unrepresentable: [`Self::parse`] is the only
/// constructor and refuses it. Bools only -- no identity, no key material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MuteChange([Option<bool>; 2]);

/// Why a kind-keyed REQUEST list (`MuteRequest.changes`,
/// `ServerMuteRequest.changes`, `UnmuteRequest.media_kinds`) is refused WHOLE.
/// Mapped onto the counted `MuteRejection` reasons in `media_signaling`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestListRefusal {
    /// The list is empty: the request names no kind.
    NoKindNamed,
    /// An entry is malformed or unserved, or a kind is named twice.
    List(KindListRefusal),
}

impl std::fmt::Display for RequestListRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoKindNamed => f.write_str("request names no media kind (empty list)"),
            Self::List(KindListRefusal::Duplicate) => {
                f.write_str("request names a media kind twice")
            }
            Self::List(KindListRefusal::Kind(KindRefusal::Unspecified)) => {
                f.write_str("request names an unspecified or unknown media kind")
            }
            Self::List(KindListRefusal::Kind(KindRefusal::Unsupported)) => {
                f.write_str("request names a media kind MC does not serve")
            }
        }
    }
}

impl std::error::Error for RequestListRefusal {}

/// A request naming a kind its subject does not publish. The whole request is
/// refused (ADR-0036 §5 amendment 2026-10-25: never a partial apply).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotPublished;

/// Refuse an empty list; otherwise hand the parsed per-kind values on.
fn non_empty<T>(parsed: [Option<T>; 2]) -> Result<[Option<T>; 2], RequestListRefusal> {
    if parsed.iter().all(Option::is_none) {
        return Err(RequestListRefusal::NoKindNamed);
    }
    Ok(parsed)
}

impl MuteChange {
    /// Parse a request's `repeated MuteChange changes`.
    ///
    /// # Errors
    ///
    /// [`RequestListRefusal`] for an empty, malformed, unserved or duplicated
    /// list.
    pub fn parse(
        changes: &[proto_gen::dark_tower::signaling::v1::MuteChange],
    ) -> Result<Self, RequestListRefusal> {
        parse_kind_list(changes, |c| (c.media_kind, c.muted))
            .map_err(RequestListRefusal::List)
            .and_then(non_empty)
            .map(Self)
    }

    /// From per-kind asks laid out by [`MediaKindLabel::index`] (MC-internal:
    /// the reconnect declaration, tests). `None` if no kind is named.
    #[must_use]
    pub fn from_asks(asks: [Option<bool>; 2]) -> Option<Self> {
        non_empty(asks).ok().map(Self)
    }

    /// A change naming exactly one kind.
    #[must_use]
    pub fn one(kind: MediaKindLabel, muted: bool) -> Self {
        Self(MediaKindLabel::per_kind(|k| (k == kind).then_some(muted)))
    }

    /// What this request asks of `kind`: `None` means unchanged.
    #[must_use]
    pub fn get(self, kind: MediaKindLabel) -> Option<bool> {
        kind.of(self.0)
    }

    /// The kinds this request NAMES, each with its asked-for value, in
    /// [`MediaKindLabel::ALL`] order -- the counting unit of the per-kind mute
    /// counters (one increment per named kind). Never empty.
    pub fn named(self) -> impl Iterator<Item = (MediaKindLabel, bool)> {
        MediaKindLabel::ALL
            .into_iter()
            .filter_map(move |k| self.get(k).map(|muted| (k, muted)))
    }

    /// The client-mute boundary check: every named kind must be one the
    /// participant publishes, or the WHOLE request is refused. THE only
    /// constructor of a [`PublishedMuteChange`].
    ///
    /// # Errors
    ///
    /// [`NotPublished`] when any named kind is outside `publishes`.
    pub fn against(self, publishes: PublishSet) -> Result<PublishedMuteChange, NotPublished> {
        if !self.names_only(publishes) {
            return Err(NotPublished);
        }
        Ok(PublishedMuteChange(self))
    }

    /// Does this request name only kinds `publishes` contains?
    #[must_use]
    pub fn names_only(self, publishes: PublishSet) -> bool {
        publishes.covers(self.named().map(|(k, _)| k))
    }
}

impl std::fmt::Display for MuteChange {
    /// `audio=muted video=unchanged`: per kind from [`MediaKindLabel::ALL`],
    /// closed-set labels only (code-quality CR-10).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, k) in MediaKindLabel::ALL.into_iter().enumerate() {
            let asked = match self.get(k) {
                Some(true) => "muted",
                Some(false) => "unmuted",
                None => "unchanged",
            };
            write!(f, "{}{}={asked}", if i == 0 { "" } else { " " }, k.label())?;
        }
        Ok(())
    }
}

/// A client-mute change proven, at the connection boundary, to name only
/// kinds the participant publishes ([`MuteChange::against`]). The only input
/// [`ClientMediaStates::apply`] accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishedMuteChange(MuteChange);

impl PublishedMuteChange {
    /// The underlying change (for counting and the no-op short-circuit).
    #[must_use]
    pub fn change(self) -> MuteChange {
        self.0
    }
}

/// The kinds an `UnmuteRequest` asks to have lifted (R-10), parsed and
/// checked like a mute request: non-empty, served, no duplicate. A
/// [`PublishSet`] in shape (DRY F7), so the kind-set operations have one home.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestedKinds(PublishSet);

impl RequestedKinds {
    /// Parse `UnmuteRequest.media_kinds`.
    ///
    /// # Errors
    ///
    /// [`RequestListRefusal`] for an empty, malformed, unserved or duplicated
    /// list.
    pub fn parse(media_kinds: &[i32]) -> Result<Self, RequestListRefusal> {
        parse_kind_list(media_kinds, |k| (*k, ()))
            .map_err(RequestListRefusal::List)
            .and_then(non_empty)
            .map(|p| Self(PublishSet(p.map(|k| k.is_some()))))
    }

    /// From explicit kinds (tests, MC-internal).
    #[must_use]
    pub fn of(kinds: &[MediaKindLabel]) -> Self {
        Self(PublishSet::of(kinds))
    }

    /// The publish-set check: every named kind must be published.
    ///
    /// # Errors
    ///
    /// [`NotPublished`] when any named kind is outside `publishes`.
    pub fn against(self, publishes: PublishSet) -> Result<Self, NotPublished> {
        if !publishes.covers(self.kinds()) {
            return Err(NotPublished);
        }
        Ok(self)
    }

    /// The kinds named, in [`MediaKindLabel::ALL`] order.
    pub fn kinds(self) -> impl Iterator<Item = MediaKindLabel> {
        self.0.kinds()
    }

    /// The named kinds that are currently server-muted (what MC relays).
    #[must_use]
    pub fn server_muted_among(self, muted: ServerMutedKinds) -> Self {
        Self(PublishSet(MediaKindLabel::per_kind(|k| {
            self.0.publishes(k) && muted.is_muted(k)
        })))
    }

    /// Is no kind named?
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0.is_empty()
    }
}

/// The outcome of merging one change into a participant's per-kind state, PER
/// KIND: whether each kind moved, and each kind's resulting state. The ONE
/// decision shape for client and server mute (it feeds the per-kind
/// `applied`/`unchanged` outcome of both mute counters).
///
/// Constructible only by [`ServerMuteAttribution::apply`] and [`ClientMediaStates::apply`];
/// read through kind-keyed accessors, so no caller can mis-index audio and
/// video.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KindDecision<S = bool> {
    changed: [bool; 2],
    state: [S; 2],
}

impl<S: Copy + PartialEq> KindDecision<S> {
    fn between(previous: [S; 2], state: [S; 2]) -> Self {
        Self {
            changed: MediaKindLabel::per_kind(|k| k.of(state) != k.of(previous)),
            state,
        }
    }

    /// Did `kind`'s state move?
    #[must_use]
    pub fn changed(self, kind: MediaKindLabel) -> bool {
        kind.of(self.changed)
    }

    /// `kind`'s state after the merge.
    #[must_use]
    pub fn state(self, kind: MediaKindLabel) -> S {
        kind.of(self.state)
    }

    /// No kind moved: a no-op (nothing broadcast, rendered or pushed).
    #[must_use]
    pub fn is_unchanged(self) -> bool {
        !self.changed.contains(&true)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use proto_gen::dark_tower::signaling::v1 as wire;

    const A: i32 = MediaKind::Audio as i32;
    const V: i32 = MediaKind::VideoCamera as i32;
    const S: i32 = MediaKind::VideoScreen as i32;
    const U: i32 = MediaKind::Unspecified as i32;

    const AU: MediaKindLabel = MediaKindLabel::Audio;
    const VI: MediaKindLabel = MediaKindLabel::Video;

    fn kinds(raw: &[i32]) -> Result<[Option<()>; 2], KindListRefusal> {
        parse_kind_list(raw, |k| (*k, ()))
    }

    fn published(media_kind: i32, client_muted: bool) -> wire::PublishedKind {
        wire::PublishedKind {
            media_kind,
            client_muted,
        }
    }

    fn change(media_kind: i32, muted: bool) -> wire::MuteChange {
        wire::MuteChange { media_kind, muted }
    }

    /// The parser table (Gate 1 test): every served order is accepted, and
    /// every malformed entry fails with the right refusal, never deduplicated.
    #[test]
    fn the_kind_list_parser_accepts_served_kinds_and_refuses_the_rest() {
        assert_eq!(kinds(&[]), Ok([None, None]));
        assert_eq!(kinds(&[A]), Ok([Some(()), None]));
        assert_eq!(kinds(&[V]), Ok([None, Some(())]));
        assert_eq!(kinds(&[A, V]), kinds(&[V, A]), "order-insensitive");
        let unspecified = Err(KindListRefusal::Kind(KindRefusal::Unspecified));
        for bad in [&[U][..], &[4242], &[-1], &[A, U]] {
            assert_eq!(kinds(bad), unspecified, "{bad:?}");
        }
        assert_eq!(
            kinds(&[S]),
            Err(KindListRefusal::Kind(KindRefusal::Unsupported))
        );
        for dup in [&[A, A][..], &[V, V], &[A, V, A]] {
            assert_eq!(kinds(dup), Err(KindListRefusal::Duplicate), "{dup:?}");
        }
        // A conflicting duplicate is a duplicate, never last-wins.
        assert_eq!(
            parse_kind_list(&[published(A, true), published(A, false)], |p| (
                p.media_kind,
                p.client_muted
            )),
            Err(KindListRefusal::Duplicate)
        );
    }

    /// A hostile list is refused within three entries: the parser never walks
    /// it.
    #[test]
    fn a_long_hostile_list_is_refused_at_the_third_entry() {
        use std::cell::Cell;
        let seen = Cell::new(0usize);
        let hostile = std::iter::repeat([A, V]).flatten().take(1_000_000);
        let refused = parse_kind_list(hostile, |k| {
            seen.set(seen.get() + 1);
            (k, ())
        });
        assert_eq!(refused, Err(KindListRefusal::Duplicate));
        assert_eq!(seen.get(), 3);
    }

    /// CQ-5: the snapshot kind list is exactly the values the wire enum
    /// accepts, minus UNSPECIFIED, once each.
    #[test]
    fn known_wire_kinds_are_every_value_the_enum_accepts() {
        let accepted: Vec<MediaKind> = (0..64)
            .filter_map(|n| MediaKind::try_from(n).ok())
            .filter(|k| *k != MediaKind::Unspecified)
            .collect();
        assert_eq!(accepted, KNOWN_WIRE_KINDS.to_vec());
    }

    #[test]
    fn served_kind_refuses_screen_as_unsupported_and_the_rest_as_unspecified() {
        assert_eq!(served_kind(A), Ok(AU));
        assert_eq!(served_kind(V), Ok(VI));
        assert_eq!(served_kind(S), Err(KindRefusal::Unsupported));
        assert_eq!(served_kind(U), Err(KindRefusal::Unspecified));
        assert_eq!(served_kind(99), Err(KindRefusal::Unspecified));
    }

    /// DRY Gate 1: the two mappings agree wherever admission yields a kind.
    #[test]
    fn served_kind_agrees_with_the_label_projection() {
        for kind in [
            MediaKind::Unspecified,
            MediaKind::Audio,
            MediaKind::VideoCamera,
            MediaKind::VideoScreen,
        ] {
            if let Ok(served) = served_kind(kind as i32) {
                assert_eq!(MediaKindLabel::from_proto(kind), Some(served), "{kind:?}");
                assert_eq!(served.to_proto(), kind);
            }
        }
    }

    #[test]
    fn per_kind_accessors_are_consistent_with_index() {
        let mut pair = [10, 20];
        for k in MediaKindLabel::ALL {
            assert_eq!(*k.of_ref(&pair), k.of(pair));
            assert_eq!(k.of([0, 1]), k.index());
            *k.of_mut(&mut pair) += 1;
            assert_eq!(k.other().other(), k);
            assert_ne!(k.other(), k);
        }
        assert_eq!(pair, [11, 21]);
    }

    /// The declaration builds a definite state per kind, and the publish set
    /// is DERIVED from it.
    #[test]
    fn the_declaration_yields_definite_states_and_the_derived_publish_set() {
        let none = ClientMediaStates::parse_declaration(&[]).unwrap();
        assert_eq!(none, ClientMediaStates::NOT_PUBLISHING);
        assert!(none.publish_set().is_empty());

        let audio_muted = ClientMediaStates::parse_declaration(&[published(A, true)]).unwrap();
        assert_eq!(audio_muted.get(AU), ClientKindState::ClientMuted);
        assert_eq!(audio_muted.get(VI), ClientKindState::NotPublished);
        assert_eq!(audio_muted.publish_set(), PublishSet::of(&[AU]));
        assert_eq!(
            audio_muted.effective(),
            ClientMutedKinds::from_client_flags([true, false])
        );

        let both =
            ClientMediaStates::parse_declaration(&[published(V, false), published(A, false)])
                .unwrap();
        assert_eq!(both.get(AU), ClientKindState::Live);
        assert_eq!(both.get(VI), ClientKindState::Live);
        assert_eq!(both.publish_set(), PublishSet::of(&[AU, VI]));
        assert_eq!(
            both.publish_set().kinds().collect::<Vec<_>>(),
            MediaKindLabel::ALL.to_vec()
        );

        for bad in [
            vec![published(S, false)],
            vec![published(U, false)],
            vec![published(77, false)],
            vec![published(A, true), published(A, false)],
        ] {
            assert_eq!(
                ClientMediaStates::parse_declaration(&bad),
                Err(PublishKindInvalid),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn the_mc_to_wire_client_state_mapping_never_yields_unspecified() {
        for s in [
            ClientKindState::NotPublished,
            ClientKindState::Live,
            ClientKindState::ClientMuted,
        ] {
            assert_ne!(s.to_proto(), ClientMediaState::Unspecified, "{s:?}");
        }
        assert_eq!(
            ClientKindState::published(true).to_proto(),
            ClientMediaState::ClientMuted
        );
        assert_eq!(
            ClientKindState::published(false).to_proto(),
            ClientMediaState::Live
        );
    }

    /// Request parsing: a kind with no entry is UNCHANGED, an entry (true or
    /// false) is named, an empty list is refused.
    #[test]
    fn mute_change_names_only_listed_kinds_and_refuses_an_empty_list() {
        assert_eq!(MuteChange::parse(&[]), Err(RequestListRefusal::NoKindNamed));

        let audio_only = MuteChange::parse(&[change(A, true)]).unwrap();
        assert_eq!(audio_only.get(AU), Some(true));
        assert_eq!(audio_only.get(VI), None);
        assert_eq!(audio_only.named().collect::<Vec<_>>(), vec![(AU, true)]);

        // An explicit `false` is an unmute, never "unchanged".
        let video_unmute = MuteChange::parse(&[change(V, false)]).unwrap();
        assert_eq!(video_unmute.named().collect::<Vec<_>>(), vec![(VI, false)]);
        assert_eq!(video_unmute, MuteChange::one(VI, false));

        assert_eq!(
            MuteChange::parse(&[change(A, true), change(A, false)]),
            Err(RequestListRefusal::List(KindListRefusal::Duplicate))
        );
        assert_eq!(
            MuteChange::parse(&[change(S, true)]),
            Err(RequestListRefusal::List(KindListRefusal::Kind(
                KindRefusal::Unsupported
            )))
        );
        assert_eq!(MuteChange::from_asks([None, None]), None);
    }

    /// A server-mute request about one kind NEVER changes the other kind, from
    /// every starting state.
    #[test]
    fn applying_one_kind_leaves_the_other_untouched_from_every_state() {
        for current in [[false, false], [true, false], [false, true], [true, true]] {
            for asked in [true, false] {
                for kind in MediaKindLabel::ALL {
                    let mut store = attribution_of(current);
                    let d = store.apply(MuteChange::one(kind, asked), "h");
                    assert_eq!(d.state(kind), asked);
                    assert_eq!(d.changed(kind), kind.of(current) != asked);
                    let other = kind.other();
                    assert_eq!(d.state(other), other.of(current), "{current:?} {kind:?}");
                    assert!(!d.changed(other));
                }
            }
        }
    }

    /// A server-mute store with `muted` kinds imposed by "h".
    fn attribution_of(muted: [bool; 2]) -> ServerMuteAttribution {
        let mut a = ServerMuteAttribution::none();
        let _ = a.apply(
            MuteChange::from_asks(muted.map(|m| m.then_some(true)))
                .unwrap_or(MuteChange::one(AU, false)),
            "h",
        );
        a
    }

    #[test]
    fn a_repeat_is_unchanged_and_a_move_is_not() {
        let mute_both = MuteChange::from_asks([Some(true), Some(true)]).unwrap();
        assert!(attribution_of([true, true])
            .apply(mute_both, "h")
            .is_unchanged());
        let d = attribution_of([true, false]).apply(mute_both, "h");
        assert!(!d.is_unchanged());
        assert!(!d.changed(AU) && d.changed(VI));
    }

    /// The boundary check refuses the WHOLE change when ANY named kind is
    /// unpublished (inverts the R-15 "drop only unpublished kinds" test).
    #[test]
    fn against_refuses_any_unpublished_kind_whole() {
        let audio_only = PublishSet::of(&[AU]);
        let mixed = MuteChange::from_asks([Some(true), Some(true)]).unwrap();
        assert_eq!(mixed.against(audio_only), Err(NotPublished));
        assert_eq!(
            MuteChange::one(VI, false).against(audio_only),
            Err(NotPublished)
        );
        assert_eq!(
            MuteChange::one(AU, true).against(PublishSet::NONE),
            Err(NotPublished)
        );
        let ok = MuteChange::one(AU, true).against(audio_only).unwrap();
        assert_eq!(ok.change(), MuteChange::one(AU, true));
    }

    /// The client merge: a named published kind moves Live <-> ClientMuted, an
    /// unnamed kind keeps its state, a repeat is unchanged, and a NotPublished
    /// kind is never resurrected.
    #[test]
    fn the_client_merge_moves_only_named_published_kinds() {
        let states = ClientMediaStates::from_declared([Some(false), None]);
        let mute = MuteChange::one(AU, true)
            .against(states.publish_set())
            .unwrap();
        let d = states.apply(mute).unwrap();
        assert!(d.changed(AU) && !d.changed(VI));
        assert_eq!(d.state(AU), ClientKindState::ClientMuted);
        assert_eq!(d.state(VI), ClientKindState::NotPublished);

        let after = ClientMediaStates::from(d);
        assert!(
            after.apply(mute).unwrap().is_unchanged(),
            "a repeat is a no-op"
        );

        // CR-4: a change proven against a WIDER set is REFUSED loudly, never
        // counted as unchanged, and never resurrects an unpublished kind.
        let wide = MuteChange::one(VI, true)
            .against(PublishSet::of(&[AU, VI]))
            .unwrap();
        assert_eq!(after.apply(wide), Err(NotPublished));
    }

    /// CR-5: a declaration is its own change, proven by construction.
    #[test]
    fn a_declaration_as_a_change_names_exactly_its_published_kinds() {
        let declared = ClientMediaStates::from_declared([Some(true), None]);
        let change = declared.as_change().unwrap().change();
        assert_eq!(change, MuteChange::one(AU, true));
        assert!(ClientMediaStates::NOT_PUBLISHING.as_change().is_none());
        // Adopting it into the same publish set always succeeds.
        let roster = ClientMediaStates::from_declared([Some(false), None]);
        let d = roster.apply(declared.as_change().unwrap()).unwrap();
        assert!(d.changed(AU));
    }

    #[test]
    fn mute_change_displays_per_kind_from_the_closed_set() {
        assert_eq!(
            MuteChange::one(VI, true).to_string(),
            "audio=unchanged video=muted"
        );
        assert_eq!(
            MuteChange::from_asks([Some(false), Some(true)])
                .unwrap()
                .to_string(),
            "audio=unmuted video=muted"
        );
    }

    #[test]
    fn attribution_is_the_server_mute_store() {
        let mut attribution = ServerMuteAttribution::none();
        assert_eq!(attribution.muted_kinds(), ServerMutedKinds::NONE);

        let _ = attribution.apply(MuteChange::one(AU, true), "host-1");
        assert_eq!(attribution.imposer(AU), Some("host-1"));
        assert_eq!(attribution.imposer(VI), None);
        assert!(attribution.muted_kinds().is_muted(AU));

        let _ = attribution.apply(MuteChange::one(VI, true), "host-2");
        // Each kind keeps its OWN imposer.
        assert_eq!(attribution.imposer(AU), Some("host-1"));
        assert_eq!(attribution.imposer(VI), Some("host-2"));

        // An unmute of one kind never touches the other's attribution.
        let d = attribution.apply(MuteChange::one(AU, false), "host-2");
        assert!(d.changed(AU) && !d.changed(VI));
        assert_eq!(attribution.imposer(AU), None);
        assert_eq!(attribution.imposer(VI), Some("host-2"));
        for k in MediaKindLabel::ALL {
            assert_eq!(
                attribution.muted_kinds().is_muted(k),
                attribution.imposer(k).is_some()
            );
        }
    }

    #[test]
    fn requested_kinds_parse_check_and_filter() {
        assert_eq!(
            RequestedKinds::parse(&[]),
            Err(RequestListRefusal::NoKindNamed)
        );
        assert_eq!(
            RequestedKinds::parse(&[V, V]),
            Err(RequestListRefusal::List(KindListRefusal::Duplicate))
        );
        let both = RequestedKinds::parse(&[A, V]).unwrap();
        assert_eq!(both, RequestedKinds::of(&[AU, VI]));
        assert_eq!(both.against(PublishSet::of(&[AU])), Err(NotPublished));
        let camera_muted = ServerMutedKinds::from_server_flags([false, true]);
        assert_eq!(
            both.server_muted_among(camera_muted),
            RequestedKinds::of(&[VI])
        );
        assert!(RequestedKinds::of(&[AU])
            .server_muted_among(camera_muted)
            .is_empty());
    }
}
