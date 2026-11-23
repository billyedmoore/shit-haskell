{-# LANGUAGE InstanceSigs, ScopedTypeVariables #-}
class C a where
  method :: a -> Bool
instance Eq a => C [a] where
  method :: Eq a => [a] -> Bool
  method [a, b] = a == b
  method _ = False

main :: IO ()
main = print (method [True, False])
