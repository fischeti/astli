// A dynamic array or a queue holds as many elements as it is given: a
// concatenation gives an element for each part, and each element of a part
// that is itself an array; a pattern one for each item. It is indexed from
// 0, and an index past its end reads what its element type starts with.
package p;
  localparam bit [7:0] Bytes[] = {8'd3, 8'd11, 8'd59};
  localparam string Names[] = {"a", "bc"};
  localparam int Queue[$] = '{1, 2, 3, 4};
  localparam int Joined[] = {Queue, 5, Bytes[1]};
  localparam int Repeated[] = '{3{7}};
  localparam int Empty[$] = {};
  localparam int A = $size(Joined);
  localparam int B = Queue.size();
  localparam int C = Queue[2];
  localparam int D = Queue[9];
  localparam int E = $right(Empty);
  localparam bit F = 59 inside {Bytes};
  localparam string G = Names[1];
endpackage
