{-# LANGUAGE InstanceSigs, FlexibleContexts, ConstrainedClassMethods #-}
class C a where method :: a -> Bool
instance C Int where
  method :: Eq Int => Int -> Bool
  method x = x == x
main :: IO ()
main = print (method (3 :: Int))
