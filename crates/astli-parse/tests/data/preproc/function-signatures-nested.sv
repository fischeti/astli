// Nested signatures share a function body inside an ordinary class.
class driver;
    int count;
    virtual function
`ifdef WIDE
`ifdef SIGNED
        void set_value(longint value);
`else
        void set_value(longint unsigned value);
`endif
`else
        void set_value(int value);
`endif
        // This body belongs to every signature.
        count = value;
    endfunction : set_value
endclass
