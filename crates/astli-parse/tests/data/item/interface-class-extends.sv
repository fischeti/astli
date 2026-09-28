// An interface class may extend several interface classes, each a type that
// may take parameters.
interface class I3 #(type T = logic) extends I1 #(T), I2 #(T);
  pure virtual function void f(T a);
endclass
interface class I4 extends I1, I2, I3;
endclass
