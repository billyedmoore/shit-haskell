{-# LANGUAGE TypeFamilies #-}
type family Good a b where
  Good a a = Bool
  Good a b = Int
main :: IO ()
main = pure ()
