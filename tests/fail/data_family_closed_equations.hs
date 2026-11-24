{-# LANGUAGE TypeFamilies #-}

data family Result a where
  Result Int = Bool

main = pure ()
