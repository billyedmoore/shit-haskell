{-# LANGUAGE RankNTypes #-}
-- A record field of a constrained function type, strict or not, used through its selector
-- (GHC's `renderJsS :: !(JsRender doc => RenderJs doc -> JStat -> doc)`).
module Main where

class Render doc where
  text :: String -> doc

instance Render [a] where
  text s = []

newtype Doc = Doc String deriving Show

instance Render Doc where
  text = Doc

data Renderer doc = Renderer
  { renderInt :: !(Render doc => Renderer doc -> Int -> doc)
  , renderStr :: Render doc => Renderer doc -> String -> doc
  }

defaultRenderer :: Renderer doc
defaultRenderer = Renderer (\_ n -> text (show n)) (\_ s -> text s)

class ToDoc a where toDoc :: Render doc => Renderer doc -> a -> doc
instance ToDoc Int where toDoc r = renderInt r r
instance ToDoc Bool where toDoc r b = renderStr r r (show b)

main :: IO ()
main = do
  print (toDoc defaultRenderer (42 :: Int) :: Doc)
  print (toDoc defaultRenderer True :: Doc)
