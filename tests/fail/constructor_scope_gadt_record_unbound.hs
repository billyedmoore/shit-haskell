{-# LANGUAGE GADTs #-}
data T where
  T :: forall a. { field :: missing } -> T
main :: IO ()
main = pure ()
