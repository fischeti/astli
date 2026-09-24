// An assignment pattern that does not fit ends its line with `'{`, puts one
// item per line a continuation in, and closes on a line of its own. A key
// takes no space before its `:` and one after.
module m;
  assign a = '{ b , c };
  assign d = '{default : '0, x:1};
  assign e = '{ 4 { f } };
  assign g = t'{ h: 1 };
  assign req_o = '{
    a_valid: 1'b1,
    a_opcode: PutFullData,
    a_address: address_q,
    a_data: write_data_q,
    a_mask: '1,
    default: '0
  };
  kmac_pkg::err_t kmac_err = '{valid: 1'b1,
                               code: kmac_pkg::ErrIncorrectEntropyMode, info: '0};
  localparam cfg_t DefaultConfiguration = '{enable_feature_one: 1'b1, feature_two_mode: ModeFast, depth: 16};
endmodule
