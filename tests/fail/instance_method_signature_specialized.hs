{-# LANGUAGE InstanceSigs, FlexibleContexts, ConstrainedClassMethods #-}
class C a where method :: a -> b -> b
instance C Int where
  method :: Int -> Bool -> Bool
  method _ = id
main :: IO ()
main = pure ()
