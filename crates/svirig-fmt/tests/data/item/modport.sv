// A modport's ports go on its line, or one per line if they do not fit.
interface bus_if;
  modport   master( output req , input gnt );
  modport slave (input req, output gnt), monitor (input req, input gnt);
  modport a_modport_with_a_long_name (input request_valid, input request_data, output grant_q, output error_q);
endinterface
