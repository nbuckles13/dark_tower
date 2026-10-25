//! The media-kind vocabulary MC shares across its media modules (story 3).
//!
//! A LEAF module on purpose (Gate 1, code-quality CQ1): `media_signaling`
//! depends on `media_routing`, never the reverse, and both need the kind and
//! the publish set. Homing them here keeps that dependency one-way;
//! `media_signaling` re-exports [`MediaKindLabel`] so its existing call sites
//! are unchanged.
//!
//! No key material, no identity: kinds and booleans only.

use proto_gen::dark_tower::signaling::v1::MediaKind;

/// The media kind a series' subject has (`media_kind` label; story 3 OBS-A).
///
/// ANCHOR (DRY): the `media_kind` row of `docs/observability/label-taxonomy.md`
/// is the single source of truth for this value set; the test
/// `media_kind_label_matches_the_taxonomy_row` reads that row, so the two cannot
/// drift silently. The key constant is `common::observability::labels::MEDIA_KIND_LABEL`.
///
/// A TELEMETRY projection, deliberately distinct from the wire
/// `proto_gen::dark_tower::signaling::v1::MediaKind`, which has four values
/// (`UNSPECIFIED`, `AUDIO`, `VIDEO_CAMERA`, `VIDEO_SCREEN`) against this label's
/// two.
///
/// # Two wire mappings, deliberately NOT collapsed
///
/// - [`Self::from_proto`] is the LABEL projection: every labellable wire kind
///   maps to a label (screen share is `video`), so a recorder is never handed
///   an unlabelled kind.
/// - [`served_kind`] is ADMISSION: the closed set of kinds this story SERVES.
///   It refuses `VIDEO_SCREEN` (`media_kind_unsupported`), so merging the two
///   would fail open on screen share. The `served_kind_agrees_with_the_label_projection`
///   test pins that wherever admission yields a kind, the label projection
///   yields the same one.
///
/// Since story 3 MC-A this is ALSO the slot table's kind (per-kind slot
/// vectors, edges, egress ids), so there is no third kind enum.
///
/// MC and the client SDK carry this label; MH never does (ADR-0036 §7: MH is
/// type-blind).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKindLabel {
    /// `audio`.
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

    /// Select this kind's value from a per-kind pair of named fields
    /// (`kind.pick(p.audio_server_muted, p.video_server_muted)`). The ONE
    /// kind -> field mapping, so call sites never write their own `match`.
    #[must_use]
    pub fn pick<T>(self, audio: T, video: T) -> T {
        match self {
            Self::Audio => audio,
            Self::Video => video,
        }
    }

    /// This kind's entry of a per-kind `[T; 2]` laid out by [`Self::index`]
    /// (as built by [`Self::per_kind`]). Infallible: destructures, never
    /// indexes.
    #[must_use]
    pub fn of<T>(self, per_kind: [T; 2]) -> T {
        let [audio, video] = per_kind;
        self.pick(audio, video)
    }

    /// This kind's entry of a per-kind `[T; 2]`, by reference. Infallible:
    /// destructures, never indexes (`clippy::indexing_slicing`).
    #[must_use]
    pub fn of_ref<T>(self, per_kind: &[T; 2]) -> &T {
        let [audio, video] = per_kind;
        self.pick(audio, video)
    }

    /// This kind's entry of a per-kind `[T; 2]`, mutably. Infallible.
    pub fn of_mut<T>(self, per_kind: &mut [T; 2]) -> &mut T {
        let [audio, video] = per_kind;
        self.pick(audio, video)
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
            // `content_kind="slides"` in the taxonomy. `content_kind` is
            // declared but not carried (content share is out of story 3's
            // scope), so a screen slot is labelled `video` and the split is
            // left to that label when it is carried.
            MediaKind::VideoScreen => Some(Self::Video),
            // Rejected at every parse boundary that admits a kind; it has no
            // honest label.
            MediaKind::Unspecified => None,
        }
    }
}

/// Why a wire kind is not a kind MC serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindRefusal {
    /// A real kind this story does not serve (`VIDEO_SCREEN`).
    Unsupported,
    /// `MEDIA_KIND_UNSPECIFIED`, or an i32 outside the enum (the enum is open
    /// on the wire, so an unknown number decodes as an integer).
    Unspecified,
}

/// ADMISSION: the closed set of kinds MC serves, from a raw wire value.
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

/// What a participant publishes, fixed for its membership (story 3 R-2).
///
/// Constructible only through [`Self::parse`] (or [`Self::of`] for tests and
/// MC-internal construction from already-typed kinds), so a set holding an
/// unserved kind or a duplicate is unrepresentable. Equality is SET equality:
/// order on the wire is irrelevant, which is what the reconnect comparison
/// needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PublishSet([bool; 2]);

/// A `publish_kinds` list MC refuses (join token `publish_kind_invalid`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishKindInvalid;

impl PublishSet {
    /// Receive-only: publishes nothing.
    pub const NONE: Self = Self([false; 2]);

    /// Parse the wire `publish_kinds`.
    ///
    /// Fails on the FIRST unserved or duplicate value, so a long hostile list
    /// costs at most three iterations and allocates nothing (Gate 1 security):
    /// a third valid entry is necessarily a duplicate.
    ///
    /// # Errors
    ///
    /// [`PublishKindInvalid`] for `UNSPECIFIED`, `VIDEO_SCREEN`, an unknown
    /// value or a duplicate — rejected, never ignored or deduplicated.
    pub fn parse(raw: &[i32]) -> Result<Self, PublishKindInvalid> {
        let mut set = [false; 2];
        for value in raw {
            let kind = served_kind(*value).map_err(|_| PublishKindInvalid)?;
            let slot = kind.of_mut(&mut set);
            if *slot {
                return Err(PublishKindInvalid);
            }
            *slot = true;
        }
        Ok(Self(set))
    }

    /// A set publishing exactly `kinds` (duplicates collapse; MC-internal use).
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

    /// From the roster's per-kind server-mute flags (`audio_server_muted`,
    /// `video_server_muted`), laid out by [`MediaKindLabel::index`].
    #[must_use]
    pub const fn from_server_flags(flags: [bool; 2]) -> Self {
        Self(flags)
    }

    /// Is `kind` server-muted?
    #[must_use]
    pub fn is_muted(self, kind: MediaKindLabel) -> bool {
        kind.of(self.0)
    }
}

/// A source's CLIENT-mute flags, per kind (story 3 R-17). See
/// [`ServerMutedKinds`] for why the two are distinct types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClientMutedKinds([bool; 2]);

impl ClientMutedKinds {
    /// Client-muted on neither kind.
    pub const NONE: Self = Self([false; 2]);

    /// From per-kind client-mute flags, laid out by [`MediaKindLabel::index`].
    /// The roster builds this ONLY through [`ClientMuteFacts::effective`].
    #[must_use]
    pub const fn from_client_flags(flags: [bool; 2]) -> Self {
        Self(flags)
    }

    /// Is `kind` client-muted?
    #[must_use]
    pub fn is_muted(self, kind: MediaKindLabel) -> bool {
        kind.of(self.0)
    }
}

/// What MC KNOWS of a participant's client mute, per kind (story 3 R-15 fix):
/// `None` until that participant has reported the kind, then its last
/// reported value. Laid out by [`MediaKindLabel::index`].
///
/// Two readers, deliberately different:
/// - the WIRE carries the facts as they are (`Participant` /
///   `ParticipantMuteUpdate` `*_client_muted`, set iff `Some`), so a peer shows
///   "unknown" rather than an unmuted MC never heard;
/// - MEDIA POLICY (PAUSED disposition, slot state) reads [`Self::effective`],
///   THE one place an unknown kind is treated as not muted -- which is what MC
///   has always done before a participant's first report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClientMuteFacts([Option<bool>; 2]);

impl ClientMuteFacts {
    /// Nothing reported for either kind (a fresh joiner).
    pub const UNKNOWN: Self = Self([None; 2]);

    /// From per-kind facts laid out by [`MediaKindLabel::index`].
    #[must_use]
    pub const fn from_facts(facts: [Option<bool>; 2]) -> Self {
        Self(facts)
    }

    /// The per-kind facts, laid out by [`MediaKindLabel::index`] (the shape
    /// [`MuteChange::apply`] merges against).
    #[must_use]
    pub const fn facts(self) -> [Option<bool>; 2] {
        self.0
    }

    /// What MC knows of `kind`: `None` = never reported.
    #[must_use]
    pub fn get(self, kind: MediaKindLabel) -> Option<bool> {
        kind.of(self.0)
    }

    /// The media-policy projection: an unreported kind is not muted. THE ONE
    /// `unwrap_or(false)` for client mute in MC.
    #[must_use]
    pub fn effective(self) -> ClientMutedKinds {
        ClientMutedKinds::from_client_flags(MediaKindLabel::per_kind(|k| {
            self.get(k).unwrap_or(false)
        }))
    }
}

impl From<KindDecision<Option<bool>>> for ClientMuteFacts {
    fn from(decision: KindDecision<Option<bool>>) -> Self {
        Self(decision.state)
    }
}

/// One mute request's per-kind ask (story 3 R-15, R-17): for each kind, `None`
/// (ABSENT on the wire: that kind is UNCHANGED) or `Some(muted)`.
///
/// THE ONE PARSER for both `MuteRequest` (client mute) and `ServerMuteRequest`
/// (server mute): both connection paths build one with [`Self::parse`] and hand
/// it, as-is, to the meeting actor, which merges it against the ROSTER (never
/// against a connection cache) with [`Self::apply`].
///
/// A request naming NO kind is unrepresentable: [`Self::parse`] is the only
/// constructor and refuses it ([`NoKindNamed`]), so the actor can never be
/// handed one. Bools only -- no identity, no key material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MuteChange([Option<bool>; 2]);

/// A mute request that names no kind: both per-kind fields absent. Refused
/// with a generic client message and counted on the kind-free
/// `mc_media_mute_requests_rejected_total{reason="no_kind_named"}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoKindNamed;

impl std::fmt::Display for NoKindNamed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("mute request names no media kind (both per-kind fields absent)")
    }
}

impl std::error::Error for NoKindNamed {}

impl MuteChange {
    /// Parse the wire's two `optional bool` fields.
    ///
    /// # Errors
    ///
    /// [`NoKindNamed`] when both are absent.
    pub fn parse(audio: Option<bool>, video: Option<bool>) -> Result<Self, NoKindNamed> {
        if audio.is_none() && video.is_none() {
            return Err(NoKindNamed);
        }
        Ok(Self(MediaKindLabel::per_kind(|k| k.pick(audio, video))))
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

    /// This request restricted to the kinds `publishes` contains, or `None` if
    /// it names no published kind. A client-mute fact exists only for a kind
    /// the participant publishes (story 3 R-15 fix): an unpublished kind is
    /// ignored, never recorded.
    #[must_use]
    pub fn published_only(self, publishes: PublishSet) -> Option<Self> {
        let kept = MediaKindLabel::per_kind(|k| self.get(k).filter(|_| publishes.publishes(k)));
        (kept != [None; 2]).then_some(Self(kept))
    }

    /// The kinds this request NAMES that `publishes` does not contain.
    pub fn unpublished(self, publishes: PublishSet) -> impl Iterator<Item = MediaKindLabel> {
        self.named()
            .map(|(k, _)| k)
            .filter(move |k| !publishes.publishes(*k))
    }

    /// Merge this request into `current` (per kind, laid out by
    /// [`MediaKindLabel::index`]): a named kind takes its asked-for value, an
    /// unnamed kind keeps its current value. The ONE merge both mute writers
    /// use, so "a request about one kind never changes the other" is a property
    /// of this function.
    ///
    /// Generic over the per-kind state: server mute merges into `bool` flags,
    /// client mute into [`ClientMuteFacts`]' `Option<bool>` facts (where a first
    /// report `None -> Some(_)` is a change of the FACT -- see
    /// [`KindDecision::effective_moved`] for the media question).
    #[must_use]
    pub fn apply<S: Copy + PartialEq + From<bool>>(self, current: [S; 2]) -> KindDecision<S> {
        let state = MediaKindLabel::per_kind(|k| self.get(k).map_or(k.of(current), S::from));
        KindDecision {
            changed: MediaKindLabel::per_kind(|k| k.of(state) != k.of(current)),
            state,
            previous: current,
        }
    }
}

/// The outcome of merging one [`MuteChange`] into a participant's flags, PER
/// KIND: whether each kind moved, and each kind's resulting state. The ONE
/// decision shape for client and server mute (it feeds the per-kind
/// `applied`/`unchanged` outcome of both mute counters).
///
/// Constructible only by [`MuteChange::apply`]; read through kind-keyed
/// accessors, so no caller can mis-index audio and video.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KindDecision<S = bool> {
    changed: [bool; 2],
    state: [S; 2],
    previous: [S; 2],
}

impl<S: Copy> KindDecision<S> {
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

    /// Neither kind moved: a no-op (nothing broadcast, rendered or pushed).
    #[must_use]
    pub fn is_unchanged(self) -> bool {
        !self.changed.contains(&true)
    }
}

impl KindDecision<Option<bool>> {
    /// The MEDIA question on a client-mute decision: did the media-policy
    /// projection ([`ClientMuteFacts::effective`]) move? A first report of
    /// `false` changes the fact (so it is not [`Self::is_unchanged`] and the
    /// fact fans out) but not the projection, so nothing re-renders.
    #[must_use]
    pub fn effective_moved(self) -> bool {
        ClientMuteFacts::from_facts(self.previous).effective()
            != ClientMuteFacts::from_facts(self.state).effective()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const A: i32 = MediaKind::Audio as i32;
    const V: i32 = MediaKind::VideoCamera as i32;
    const S: i32 = MediaKind::VideoScreen as i32;
    const U: i32 = MediaKind::Unspecified as i32;

    #[test]
    fn publish_set_accepts_the_closed_set_in_any_order() {
        assert_eq!(PublishSet::parse(&[]).unwrap(), PublishSet::NONE);
        assert!(PublishSet::parse(&[]).unwrap().is_empty());
        let av = PublishSet::parse(&[A, V]).unwrap();
        assert_eq!(av, PublishSet::parse(&[V, A]).unwrap(), "set equality");
        assert!(av.publishes(MediaKindLabel::Audio) && av.publishes(MediaKindLabel::Video));
        let a = PublishSet::parse(&[A]).unwrap();
        assert!(a.publishes(MediaKindLabel::Audio) && !a.publishes(MediaKindLabel::Video));
        assert_eq!(av.kinds().collect::<Vec<_>>(), MediaKindLabel::ALL.to_vec());
    }

    #[test]
    fn publish_set_rejects_screen_unspecified_unknown_and_duplicates() {
        for bad in [&[S][..], &[U], &[4242], &[A, A], &[V, V], &[A, V, A], &[-1]] {
            assert_eq!(PublishSet::parse(bad), Err(PublishKindInvalid), "{bad:?}");
        }
    }

    #[test]
    fn served_kind_refuses_screen_as_unsupported_and_the_rest_as_unspecified() {
        assert_eq!(served_kind(A), Ok(MediaKindLabel::Audio));
        assert_eq!(served_kind(V), Ok(MediaKindLabel::Video));
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
            *k.of_mut(&mut pair) += 1;
            assert_eq!(k.other().other(), k);
            assert_ne!(k.other(), k);
        }
        assert_eq!(pair, [11, 21]);
    }

    const AU: MediaKindLabel = MediaKindLabel::Audio;
    const VI: MediaKindLabel = MediaKindLabel::Video;

    /// R-15/R-17 parsing: an absent field is UNCHANGED, a present one (true or
    /// false) is named, and a request naming neither kind is refused.
    #[test]
    fn mute_change_names_only_present_kinds_and_refuses_both_absent() {
        assert_eq!(MuteChange::parse(None, None), Err(NoKindNamed));

        let audio_only = MuteChange::parse(Some(true), None).unwrap();
        assert_eq!(audio_only.get(AU), Some(true));
        assert_eq!(audio_only.get(VI), None);
        assert_eq!(audio_only.named().collect::<Vec<_>>(), vec![(AU, true)]);

        // An explicit `false` is an unmute, never "unchanged".
        let video_unmute = MuteChange::parse(None, Some(false)).unwrap();
        assert_eq!(video_unmute.named().collect::<Vec<_>>(), vec![(VI, false)]);

        let both = MuteChange::parse(Some(false), Some(true)).unwrap();
        assert_eq!(
            both.named().collect::<Vec<_>>(),
            vec![(AU, false), (VI, true)]
        );
    }

    /// A request about one kind NEVER changes the other kind, from every
    /// starting state.
    #[test]
    fn applying_one_kind_leaves_the_other_untouched_from_every_state() {
        for current in [[false, false], [true, false], [false, true], [true, true]] {
            for asked in [true, false] {
                for kind in MediaKindLabel::ALL {
                    let change = MuteChange::parse(
                        kind.pick(Some(asked), None),
                        kind.pick(None, Some(asked)),
                    )
                    .unwrap();
                    let d = change.apply(current);
                    assert_eq!(d.state(kind), asked);
                    assert_eq!(d.changed(kind), kind.of(current) != asked);
                    let other = kind.other();
                    assert_eq!(d.state(other), other.of(current), "{current:?} {kind:?}");
                    assert!(!d.changed(other));
                }
            }
        }
    }

    /// Client mute merges into FACTS: a first report (either value) is a fact
    /// change; only a report that moves the `unwrap_or(false)` projection is a
    /// media change; a repeat is neither; an unnamed kind stays unknown.
    #[test]
    fn a_client_fact_decision_separates_the_wire_question_from_the_media_one() {
        let unknown = ClientMuteFacts::UNKNOWN.facts();
        let first_false = MuteChange::parse(Some(false), None).unwrap().apply(unknown);
        assert!(!first_false.is_unchanged(), "None -> Some(false) fans out");
        assert!(!first_false.effective_moved(), "...but re-renders nothing");
        assert_eq!(first_false.state(AU), Some(false));
        assert_eq!(first_false.state(VI), None, "unnamed kind stays unknown");

        let first_true = MuteChange::parse(None, Some(true)).unwrap().apply(unknown);
        assert!(!first_true.is_unchanged() && first_true.effective_moved());

        let known = ClientMuteFacts::from(first_true).facts();
        let repeat = MuteChange::parse(None, Some(true)).unwrap().apply(known);
        assert!(repeat.is_unchanged() && !repeat.effective_moved());
        assert_eq!(
            ClientMuteFacts::from(first_true).effective(),
            ClientMutedKinds::from_client_flags([false, true])
        );
    }

    /// An unpublished kind is dropped from a client-mute request; a published
    /// kind in the same request survives; a request naming only unpublished
    /// kinds is nothing.
    #[test]
    fn a_change_restricted_to_the_publish_set_drops_only_unpublished_kinds() {
        let audio_only = PublishSet::of(&[AU]);
        let mixed = MuteChange::parse(Some(true), Some(true)).unwrap();
        let kept = mixed.published_only(audio_only).unwrap();
        assert_eq!((kept.get(AU), kept.get(VI)), (Some(true), None));
        assert_eq!(mixed.unpublished(audio_only).collect::<Vec<_>>(), vec![VI]);
        let video_only = MuteChange::parse(None, Some(false)).unwrap();
        assert_eq!(video_only.published_only(audio_only), None);
        assert_eq!(
            video_only.published_only(PublishSet::of(&[AU, VI])),
            Some(video_only)
        );
        assert_eq!(mixed.unpublished(PublishSet::of(&[AU, VI])).count(), 0);
    }

    #[test]
    fn a_repeat_is_unchanged_and_a_move_is_not() {
        let mute_both = MuteChange::parse(Some(true), Some(true)).unwrap();
        assert!(mute_both.apply([true, true]).is_unchanged());
        let d = mute_both.apply([true, false]);
        assert!(!d.is_unchanged());
        assert!(!d.changed(AU) && d.changed(VI));
    }
}
