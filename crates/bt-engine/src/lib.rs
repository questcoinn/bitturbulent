//! BitTorrent engine: metadata, trackers, peer protocol, storage and the
//! per-torrent download loop. Talks to the session layer only through
//! its public API and event channel.
