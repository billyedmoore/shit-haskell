{-# LANGUAGE UnboxedSums, UnboxedTuples, MagicHash #-}
-- Compact unboxed sum syntax where `#|` lexes as a single operator token (`(#|a #)` and `(#(##)| #)`).

f :: Int -> (# Int | Bool #)
f 0 = (#| True #)
f n = (# n | #)

g :: (# (# #) | Int | Bool #) -> String
g (#(##)| | #) = "unit"
g (#| n | #) = "int:" ++ show n
g (#| | b #) = "bool:" ++ show b

main :: IO ()
main = do
  case f 0 of
    (# n | #) -> print n
    (#| b #) -> print b
  case f 42 of
    (# n | #) -> print n
    (#| b #) -> print b
  putStrLn (g (#(##)| | #))
  putStrLn (g (#| 7 | #))
  putStrLn (g (#| | False #))
