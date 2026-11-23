{-# LANGUAGE TypeFamilies #-}
data family Good a b
data instance Good a a = Good
main :: IO ()
main = pure ()
