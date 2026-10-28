// `$sformatf` writes each value as `$display` would: an integer as wide as
// its width may need unless the format says otherwise, a decimal padded
// with spaces and the other bases with zeros, a real as C's `printf`.
// `%m` names the instance, which a body shared by instances cannot know.
package p;
  localparam int X = 3;
  localparam string A = $sformatf("dma_%0d_%0d", X, 12);
  localparam string B = $sformatf("[%d]", 8'd7);
  localparam string C = $sformatf("[%d]", -5);
  localparam string D = $sformatf("%h %x %o %b", 8'hA5, 12'h3, 6'o17, 4'b0101);
  localparam string E = $sformatf("%0h %4d|%-4d|", 8'h0F, 42, 42);
  localparam string F = $sformatf("%b %h %d %d", 4'b1x0z, 8'hzz, 4'bxxxx, 4'b10x1);
  localparam string G = $sformatf("%s-%5s %c 100%%", "ab", "cd", 8'd65);
  localparam string H = $sformatf("%f %.2f %e %08.3f", 1.5, 2.0, 12345.678, -3.14159);
  localparam string I = $sformatf("%m");
  localparam string J = $sformatf("%0d", X, X);
endpackage
