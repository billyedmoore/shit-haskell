{-# LANGUAGE InstanceSigs, FlexibleContexts, ConstrainedClassMethods #-}
class C a where method :: a -> b -> Bool
instance C Int where
  method :: Eq b => Int -> b -> Bool
  method _ x = x == x
main :: IO ()
main = pure ()
