// Alternative signatures share a function prefix and body.
class c;
    virtual function
`ifdef WIDE
    void set_value(longint value);
`else
    void set_value(int value);
`endif
    endfunction
endclass
