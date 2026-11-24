{-# LANGUAGE TypeFamilies #-}
type family Broken a a
main :: IO ()
main = pure ()
