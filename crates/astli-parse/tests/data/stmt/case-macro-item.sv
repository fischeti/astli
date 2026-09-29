// A macro call among a case's items that is followed by no `:`, `,` or
// operator writes whole items, as a `default` does, so it stands on its
// own; one before a `:` is an item's value.
module m;
  always_comb begin
    case (s)
      2'd0:          x = 1'b0;
      `ITEM_ONE:     x = 1'b1;
      `ITEM(2), 2'd3: x = 1'b0;
      `DEFAULT_ITEM
    endcase
    case (s)
      `ITEMS(a, b)
      `A + 1:        x = 1'b1;
      default:       x = 1'b0;
    endcase
  end
endmodule
