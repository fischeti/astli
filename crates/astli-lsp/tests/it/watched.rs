//! The files on disk a design is found from, watched where the client can.

use expect_test::expect;
use serde_json::{Value, json};

use crate::support::{Client, loaded, uri};

/// What a client that watches files for the server says of itself.
fn watching() -> Value {
    json!({ "workspace": { "didChangeWatchedFiles": { "dynamicRegistration": true } } })
}

#[test]
fn the_server_asks_a_client_that_can_to_watch_files() {
    let (mut client, _) = Client::initialize(&[], watching());
    // Answered after the registration is sent.
    let _ = client.request("astli/unknown", Value::Null);
    expect![[r#"
        [
            (
                "client/registerCapability",
                Object {
                    "registrations": Array [
                        Object {
                            "id": String("astli/watching"),
                            "method": String("workspace/didChangeWatchedFiles"),
                            "registerOptions": Object {
                                "watchers": Array [
                                    Object {
                                        "globPattern": String("**/astli.toml"),
                                    },
                                    Object {
                                        "globPattern": String("**/*.{sv,svh}"),
                                    },
                                ],
                            },
                        },
                    ],
                },
            ),
        ]
    "#]]
    .assert_debug_eq(&client.asked);
    client.finish();

    // One that cannot is asked nothing.
    let mut client = Client::start(&[], false);
    let _ = client.request("astli/unknown", Value::Null);
    assert!(client.asked.is_empty());
    client.finish();
}

#[test]
fn a_change_on_disk_loads_again_what_read_it() {
    let (mut client, _) = Client::initialize(&["a.sv", "b.sv"], watching());
    client.open(&uri("one/a.sv"), "module a;");
    assert!(client.load().discover);
    client.logged(loaded);
    let changed = |name: &str, kind: u32| {
        let changes = json!([{ "uri": uri(name), "type": kind }]);
        client.notify(
            "workspace/didChangeWatchedFiles",
            json!({ "changes": changes }),
        );
    };

    // A manifest anywhere finds the design again.
    changed("astli.toml", 2);
    assert!(client.load().discover);
    // So does a source created or deleted.
    changed("one/c.sv", 1);
    assert!(client.load().discover);
    // A source changed that the design read, though it does not exist
    // here, loads it again as it was found.
    changed("one/b.sv", 2);
    assert!(!client.load().discover);
    // One open is read from its buffer, which the disk does not change.
    changed("one/a.sv", 2);
    assert!(client.idle());
    client.finish();
}
