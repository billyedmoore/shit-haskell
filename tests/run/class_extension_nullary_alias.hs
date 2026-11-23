{-# LANGUAGE Haskell2010, NullaryTypeClasses #-}

class Empty
instance Empty

main :: IO ()
main = pure ()
