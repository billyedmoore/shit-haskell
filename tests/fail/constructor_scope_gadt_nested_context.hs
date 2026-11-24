{-# LANGUAGE GADTs #-}
data T where
  T :: forall a. Show a => (Eq a => a -> T)
main :: IO ()
main = pure ()
