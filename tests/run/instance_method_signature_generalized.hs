{-# LANGUAGE InstanceSigs, FlexibleContexts, ConstrainedClassMethods #-}
class C a where method :: a -> a
instance C Int where
  method :: b -> b
  method = id
main :: IO ()
main = print (method (3 :: Int))
