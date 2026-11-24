{-# LANGUAGE TypeFamilies #-}
data family T a
data instance T Int = Mk missing

main :: IO ()
main = pure ()
