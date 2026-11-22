{-# LANGUAGE TypeFamilies #-}

type family Result a where
  Other Int = Bool

main = pure ()
