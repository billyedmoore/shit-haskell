class C a where
  f :: a -> Bool

instance C [b] where
  f _ = True

main :: IO ()
main = print (f [0 :: Int])
