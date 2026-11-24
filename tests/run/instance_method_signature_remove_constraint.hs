{-# LANGUAGE InstanceSigs, FlexibleContexts, ConstrainedClassMethods #-}
class C a where method :: Eq b => a -> b -> Bool
instance C Int where
  method :: Int -> b -> Bool
  method _ _ = True
main :: IO ()
main = print (method (3 :: Int) False)
