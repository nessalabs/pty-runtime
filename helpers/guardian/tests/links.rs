//! Real socket contracts for bounded guardian status delivery.
use crate::{
    links::Links,
    protocol::{Channel, Frame, Kind},
};
use std::{io::Write, net::Shutdown, os::unix::net::UnixStream};

fn connected() -> (Links, Channel, Channel) {
    let (owner, owner_remote) = UnixStream::pair().unwrap();
    let (peer, peer_remote) = UnixStream::pair().unwrap();
    (
        Links::new(owner, peer, 41).unwrap(),
        Channel::new(owner_remote, 41).unwrap(),
        Channel::new(peer_remote, 41).unwrap(),
    )
}

#[test]
fn notification_reaches_both_peers_once_with_the_original_values() {
    let (mut links, mut owner, mut peer) = connected();
    links.notify(Kind::Started, [1, 2, 3, 4]);
    links.notify(Kind::Started, [1, 2, 3, 4]);
    links.notify(Kind::Started, [9; 4]);
    links.flush();
    for remote in [&mut owner, &mut peer] {
        assert_eq!(
            remote.receive().unwrap(),
            Some(Frame::new(Kind::Started, 41, [1, 2, 3, 4]))
        );
        assert_eq!(remote.receive().unwrap(), None);
    }
    assert!(links.owner.healthy());
    assert!(links.peer.healthy());
    assert_ne!(links.owner.fd(), links.peer.fd());
}

#[test]
fn successor_and_finish_allow_new_handoffs_without_duplicate_delivery() {
    let (mut links, mut owner, mut peer) = connected();
    for generation in [10, 11] {
        for kind in [Kind::Finish, Kind::Successor] {
            links.notify(kind, [generation; 4]);
            links.notify(kind, [generation; 4]);
        }
        links.pause(&[], 0);
        for remote in [&mut owner, &mut peer] {
            for kind in [Kind::Finish, Kind::Successor] {
                assert_eq!(
                    remote.receive().unwrap(),
                    Some(Frame::new(kind, 41, [generation; 4]))
                );
            }
            assert_eq!(remote.receive().unwrap(), None);
        }
    }
}

#[test]
fn more_than_one_channel_queue_of_distinct_statuses_is_delivered_without_loss() {
    let (mut links, mut owner, mut peer) = connected();
    let kinds = [
        Kind::Hello,
        Kind::Started,
        Kind::Admitted,
        Kind::WorkloadExit,
        Kind::StartFailed,
        Kind::Fault,
        Kind::Retiring,
        Kind::Terminate,
        Kind::Kill,
        Kind::Release,
        Kind::Finish,
        Kind::Abort,
        Kind::Successor,
        Kind::Execute,
        Kind::SignalApplied,
        Kind::Escalated,
    ];
    for kind in kinds {
        links.notify(kind, [kind as i32; 4]);
    }
    // Each flush admits at most the channel's eight-record bound. The second
    // flush must retain and deliver the remainder rather than dropping it.
    for chunk in kinds.chunks(8) {
        links.flush();
        for remote in [&mut owner, &mut peer] {
            for &kind in chunk {
                assert_eq!(
                    remote.receive().unwrap(),
                    Some(Frame::new(kind, 41, [kind as i32; 4]))
                );
            }
            assert_eq!(remote.receive().unwrap(), None);
        }
    }
}

#[test]
fn inbox_preserves_endpoint_identity_and_healthy_empty_channels() {
    let (mut links, mut owner, mut peer) = connected();
    assert_eq!(links.inbox(), ([None; 8], [None; 8]));
    let terminate = Frame::new(Kind::Terminate, 41, [0; 4]);
    let kill = Frame::new(Kind::Kill, 41, [0; 4]);
    owner.enqueue(terminate).unwrap();
    peer.enqueue(kill).unwrap();
    owner.flush().unwrap();
    peer.flush().unwrap();
    let mut expected_owner = [None; 8];
    expected_owner[0] = Some(terminate);
    let mut expected_peer = [None; 8];
    expected_peer[0] = Some(kill);
    assert_eq!(links.inbox(), (expected_owner, expected_peer));
    assert_eq!(links.inbox(), ([None; 8], [None; 8]));
    assert!(links.owner.healthy());
    assert!(links.peer.healthy());
}

#[test]
fn owner_eof_does_not_disable_peer_status_delivery() {
    let (mut links, owner, mut peer) = connected();
    drop(owner);
    assert_eq!(links.inbox(), ([None; 8], [None; 8]));
    assert!(!links.owner.healthy());
    assert!(links.peer.healthy());
    assert_eq!(links.pollfds()[0].fd, -1);
    links.fault();
    links.flush();
    assert_eq!(
        peer.receive().unwrap(),
        Some(Frame::new(Kind::Fault, 41, [0; 4]))
    );
    assert_eq!(links.inbox(), ([None; 8], [None; 8]));
}

#[test]
fn malformed_owner_frame_is_isolated_from_the_peer() {
    let (owner, mut remote) = UnixStream::pair().unwrap();
    let (peer, peer_remote) = UnixStream::pair().unwrap();
    let mut links = Links::new(owner, peer, 41).unwrap();
    remote.write_all(&[0; 32]).unwrap();
    assert_eq!(links.inbox(), ([None; 8], [None; 8]));
    assert!(!links.owner.healthy());
    assert!(links.peer.healthy());
    let mut peer_remote = Channel::new(peer_remote, 41).unwrap();
    links.fault();
    links.flush();
    assert_eq!(
        peer_remote.receive().unwrap(),
        Some(Frame::new(Kind::Fault, 41, [0; 4]))
    );
}

#[test]
fn failed_write_still_allows_queued_final_status_to_be_received() {
    let (owner, remote) = UnixStream::pair().unwrap();
    let (peer, _peer_remote) = UnixStream::pair().unwrap();
    let shutdown = remote.try_clone().unwrap();
    let mut remote = Channel::new(remote, 41).unwrap();
    let mut links = Links::new(owner, peer, 41).unwrap();
    let retiring = Frame::new(Kind::Retiring, 41, [0; 4]);
    remote.enqueue(retiring).unwrap();
    remote.flush().unwrap();
    shutdown.shutdown(Shutdown::Read).unwrap();
    links.fault();
    links.flush();
    assert_eq!(links.pollfds()[0].events & libc::POLLOUT, 0);
    assert_eq!(links.inbox().0[0], Some(retiring));
    assert!(links.owner.healthy());
    links.notify(Kind::Hello, [0; 4]);
    links.flush();
}
