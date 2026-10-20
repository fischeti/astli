// Only what follows the keyword says whether a type is promised or written
// out. A name alone promises one of any kind.
typedef enum e;
typedef enum logic [1:0] { A } e;
typedef class C;
typedef interface class C;
typedef C;
