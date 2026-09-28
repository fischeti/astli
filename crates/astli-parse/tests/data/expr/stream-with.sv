// In a stream, `with` takes the part of an array to stream: an index, a
// range, or a base and a width.
module m;
  initial begin
    pkt = {<< 8 {header, data with [0 +: len], crc}};
    {>> {a, b with [i], c with [lo:hi]}} = pkt;
  end
endmodule
