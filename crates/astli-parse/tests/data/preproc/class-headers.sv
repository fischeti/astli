// Alternative class headers share one member and closing keyword.
class base; endclass
interface class listener; endclass
`ifdef WITH_LISTENER
class driver extends base implements listener;
`else
class driver extends base;
`endif
    int count;
endclass
