// The first module is the file's; each after it is reported. Packages and
// interfaces do not count, nor does a module nested inside another.
package pkg;
endpackage

module first;
  module nested;
  endmodule
endmodule

interface bus_if;
endinterface

module second;
endmodule

module third;
endmodule
