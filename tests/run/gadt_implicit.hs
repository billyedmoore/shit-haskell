data T a where
  TI :: Int -> T Int
  TB :: Bool -> T Bool

eval :: T a -> a
eval (TI n) = n
eval (TB b) = b

main :: IO ()
main = print (eval (TI 3))
