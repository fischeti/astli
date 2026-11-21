//! `elab`, and the parameters a top table sets.

use crate::support::*;

/// A package whose bench `tb_cut` refuses to elaborate without a cut, and
/// passes its count to a `cut` whose port is that wide, with tables
/// building it with none and with three.
fn cuts(fixture: &Fixture) {
    fixture.file(
        "astli.toml",
        "[top.tb_cut_0]\ntop = 'tb_cut'\nparams = { NumCuts = 0 }\n\n\
         [top.tb_cut_3]\ntop = 'tb_cut'\nparams = { NumCuts = 3, Name = 'three \"cuts\"' }\n",
    );
    fixture.file(
        "tb/tb_cut.sv",
        "module cut #(parameter int N = 1) (input logic [N:0] a_i);\nendmodule\n\
         module tb_cut #(\n\
         \x20 parameter int unsigned NumCuts = 1,\n\
         \x20 parameter string Name = \"one\",\n\
         \x20 parameter type T = logic,\n\
         \x20 localparam int Width = 4\n\
         );\n\
         \x20 logic [1:0] a;\n\
         \x20 if (NumCuts == 0) begin : gen_none\n\
         \x20   $error(\"no cuts\");\n\
         \x20 end\n\
         \x20 initial $display(\"%s\", Name);\n\
         \x20 cut #(.N(NumCuts)) u_cut (.a_i(a));\n\
         endmodule\n",
    );
}

#[test]
fn a_top_table_sets_its_tops_parameters() {
    let fixture = Fixture::new("top-table-params");
    cuts(&fixture);
    let check = |args: &[&str]| driver(&fixture).args([&["check"], args].concat());

    let none = check(&["--top", "tb_cut_0"]);
    let three = check(&["--top", "tb_cut_3"]);
    let both = check(&["--top", "tb_cut_0", "--top", "tb_cut_3"]);
    // A flag's value wins over the table's.
    let flagged = check(&["--top", "tb_cut_0", "-G", "NumCuts=2"]);
    // A flag one top declares is that top's.
    let one_declares = check(&["--top", "tb_cut_3", "--top", "cut", "-G", "N=2"]);
    let lint = driver(&fixture).args(["lint", "--top", "tb_cut_3"]);
    let tree = driver(&fixture).args([
        "elab",
        "tb/tb_cut.sv",
        "--top",
        "tb_cut",
        "-G",
        "NumCuts=-1",
        "-G",
        "Name=\"x\"",
    ]);

    assert!(!none.status.success());
    assert!(stderr(&none).contains("no cuts"), "{}", stderr(&none));
    assert!(three.status.success(), "{}", stderr(&three));
    // Each table builds the one module its own way.
    assert!(!both.status.success());
    assert_eq!(
        stderr(&both).matches("Error: no cuts").count(),
        1,
        "{}",
        stderr(&both)
    );
    assert!(flagged.status.success(), "{}", stderr(&flagged));
    assert!(one_declares.status.success(), "{}", stderr(&one_declares));
    assert!(
        stderr(&lint).contains("[width-expand]") && stderr(&lint).contains("as built in `tb_cut`"),
        "{}",
        stderr(&lint)
    );
    assert!(tree.status.success(), "{}", stderr(&tree));
    assert_eq!(
        stdout(&tree),
        "tb_cut #(NumCuts = 32'd4294967295, Name = \"x\", T = logic, Width = 32'sd4)\n  \
         u_cut: cut #(N = -32'sd1)\n"
    );
}

#[test]
fn elab_writes_the_design_as_json() {
    let fixture = Fixture::new("elab-json");
    fixture.file(
        "d.sv",
        "interface bus_if #(parameter int W = 8) (input logic clk);\nendinterface\n\
         module leaf #(parameter int W = 1, parameter type T = logic, localparam int Twice = W * 2) (\n\
         \x20 input logic [W-1:0] a,\n\
         \x20 output T y,\n\
         \x20 bus_if bus\n\
         );\n\
         \x20 localparam int Inner = Twice + 1;\n\
         endmodule\n\
         module top (input logic clk);\n\
         \x20 logic [3:0] a;\n\
         \x20 logic y;\n\
         \x20 bus_if #(.W(4)) u_bus (.clk);\n\
         \x20 leaf #(.W(4)) u_arr [1:0] (.*);\n\
         \x20 for (genvar i = 0; i < 1; i++) begin : g_lane\n\
         \x20   leaf u_lane (a[i], y, u_bus);\n\
         \x20 end\n\
         \x20 if (Unknown) begin : g_maybe\n\
         \x20 end\n\
         \x20 and u_and (y, clk, clk);\n\
         \x20 absent u_absent ();\n\
         endmodule\n",
    );

    let output = driver(&fixture).args(["elab", "d.sv", "--top", "top", "--json"]);

    // The design is written whatever the files hold, and that it may
    // change is said beside it.
    assert!(
        stderr(&output).contains("note: `astli elab` is a preview"),
        "{}",
        stderr(&output)
    );
    expect_test::expect![[r#"
        {
          "tops": [
            {
              "name": "top",
              "definition": "top",
              "body": 0
            }
          ],
          "bodies": [
            {
              "id": 0,
              "definition": "top",
              "keyword": "module",
              "path": "d.sv",
              "line": 10,
              "extern": false,
              "parameters": [],
              "ports": [
                {
                  "name": "clk",
                  "interface": false,
                  "direction": "input",
                  "type": "logic",
                  "width": 1
                }
              ],
              "children": [
                {
                  "kind": "instance",
                  "name": "u_bus",
                  "definition": "bus_if",
                  "body": 1,
                  "dims": [],
                  "path": "d.sv",
                  "line": 13,
                  "connections": [
                    {
                      "port": "clk",
                      "text": "clk"
                    }
                  ]
                },
                {
                  "kind": "instance",
                  "name": "u_arr",
                  "definition": "leaf",
                  "body": 2,
                  "dims": [
                    [1, 0]
                  ],
                  "path": "d.sv",
                  "line": 14,
                  "connections": [
                    {
                      "port": "a",
                      "text": "a"
                    },
                    {
                      "port": "y",
                      "text": "y"
                    },
                    {
                      "port": "bus",
                      "text": null
                    }
                  ]
                },
                {
                  "kind": "block",
                  "name": "g_lane[0]",
                  "children": [
                    {
                      "kind": "instance",
                      "name": "u_lane",
                      "definition": "leaf",
                      "body": 3,
                      "dims": [],
                      "path": "d.sv",
                      "line": 16,
                      "connections": [
                        {
                          "port": "a",
                          "text": "a[i]"
                        },
                        {
                          "port": "y",
                          "text": "y"
                        },
                        {
                          "port": "bus",
                          "text": "u_bus"
                        }
                      ]
                    }
                  ]
                },
                {
                  "kind": "undecided",
                  "name": "g_maybe"
                },
                {
                  "kind": "gate",
                  "name": "u_and",
                  "gate": "and",
                  "path": "d.sv",
                  "line": 20
                },
                {
                  "kind": "missing",
                  "name": "u_absent",
                  "definition": "absent",
                  "path": "d.sv",
                  "line": 21
                }
              ]
            },
            {
              "id": 1,
              "definition": "bus_if",
              "keyword": "interface",
              "path": "d.sv",
              "line": 1,
              "extern": false,
              "parameters": [
                {
                  "name": "W",
                  "local": false,
                  "kind": "value",
                  "type": "bit signed [31:0]",
                  "value": "32'sd4",
                  "integer": 4,
                  "default": false
                }
              ],
              "ports": [
                {
                  "name": "clk",
                  "interface": false,
                  "direction": "input",
                  "type": "logic",
                  "width": 1
                }
              ],
              "children": []
            },
            {
              "id": 2,
              "definition": "leaf",
              "keyword": "module",
              "path": "d.sv",
              "line": 3,
              "extern": false,
              "parameters": [
                {
                  "name": "W",
                  "local": false,
                  "kind": "value",
                  "type": "bit signed [31:0]",
                  "value": "32'sd4",
                  "integer": 4,
                  "default": false
                },
                {
                  "name": "T",
                  "local": false,
                  "kind": "type",
                  "type": null,
                  "value": "logic",
                  "integer": null,
                  "default": true
                },
                {
                  "name": "Twice",
                  "local": true,
                  "kind": "value",
                  "type": "bit signed [31:0]",
                  "value": "32'sd8",
                  "integer": 8,
                  "default": false
                },
                {
                  "name": "Inner",
                  "local": true,
                  "kind": "value",
                  "type": "bit signed [31:0]",
                  "value": "32'sd9",
                  "integer": 9,
                  "default": false
                }
              ],
              "ports": [
                {
                  "name": "a",
                  "interface": false,
                  "direction": "input",
                  "type": "logic [3:0]",
                  "width": 4
                },
                {
                  "name": "y",
                  "interface": false,
                  "direction": "output",
                  "type": "logic",
                  "width": 1
                },
                {
                  "name": "bus",
                  "interface": true,
                  "direction": null,
                  "type": "bus_if",
                  "width": null
                }
              ],
              "children": []
            },
            {
              "id": 3,
              "definition": "leaf",
              "keyword": "module",
              "path": "d.sv",
              "line": 3,
              "extern": false,
              "parameters": [
                {
                  "name": "W",
                  "local": false,
                  "kind": "value",
                  "type": "bit signed [31:0]",
                  "value": "32'sd1",
                  "integer": 1,
                  "default": true
                },
                {
                  "name": "T",
                  "local": false,
                  "kind": "type",
                  "type": null,
                  "value": "logic",
                  "integer": null,
                  "default": true
                },
                {
                  "name": "Twice",
                  "local": true,
                  "kind": "value",
                  "type": "bit signed [31:0]",
                  "value": "32'sd2",
                  "integer": 2,
                  "default": true
                },
                {
                  "name": "Inner",
                  "local": true,
                  "kind": "value",
                  "type": "bit signed [31:0]",
                  "value": "32'sd3",
                  "integer": 3,
                  "default": true
                }
              ],
              "ports": [
                {
                  "name": "a",
                  "interface": false,
                  "direction": "input",
                  "type": "logic [0:0]",
                  "width": 1
                },
                {
                  "name": "y",
                  "interface": false,
                  "direction": "output",
                  "type": "logic",
                  "width": 1
                },
                {
                  "name": "bus",
                  "interface": true,
                  "direction": null,
                  "type": "bus_if",
                  "width": null
                }
              ],
              "children": []
            }
          ]
        }
    "#]]
    .assert_eq(&stdout(&output));
}

#[test]
fn a_parameter_set_as_a_top_cannot_take_it_is_refused() {
    let fixture = Fixture::new("top-params-refused");
    cuts(&fixture);

    for (flag, said) in [
        ("Nope=1", "`tb_cut` has no parameter called `Nope`"),
        ("Width=8", "`Width` of `tb_cut` is a `localparam`"),
        ("T=int", "`T` of `tb_cut` is a type parameter"),
        (
            "Name=fast",
            "`Name` of `tb_cut` is set to `fast`, which is no number or string",
        ),
        ("NumCuts", "`-G NumCuts` sets no value"),
    ] {
        let output = driver(&fixture).args(["check", "--top", "tb_cut_3", "-G", flag]);

        assert!(!output.status.success(), "{flag}");
        assert!(
            stderr(&output).contains(said),
            "{flag}: {}",
            stderr(&output)
        );
    }
    // Of several tops, one none declares is a typo.
    let none_declares =
        driver(&fixture).args(["check", "--top", "tb_cut_3", "--top", "cut", "-G", "Nope=1"]);
    assert!(!none_declares.status.success());
    assert!(
        stderr(&none_declares).contains("`-G Nope=1` sets a parameter no top declares"),
        "{}",
        stderr(&none_declares)
    );
    let no_top = driver(&fixture).args(["lint", "tb/tb_cut.sv", "-G", "NumCuts=1"]);
    assert!(!no_top.status.success());
    assert!(
        stderr(&no_top)
            .contains("`-G NumCuts=1` sets a parameter of a top, and the run builds none"),
        "{}",
        stderr(&no_top)
    );
}
