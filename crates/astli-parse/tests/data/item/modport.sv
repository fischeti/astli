// A modport names the signals it may use and how, and the subroutines it
// imports from its interface or exports to it, by name or as a prototype. An
// entry without a keyword takes the one before it.
interface i;
  modport ctrl (input a, output b), dev (input b);
  modport host (input a, import task t, export f, g,
                import function void h(int x), task u(), (* keep *) export v);
endinterface
