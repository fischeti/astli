// Nested alternative class headers share one common member and closing keyword.
class base; endclass
interface class listener; endclass
`ifdef EXTENDED
`ifdef WITH_LISTENER
class driver extends base implements listener;
`else
class driver extends base;
`endif
`else
class driver;
`endif
    int count;
endclass
