newtype State s a = State { runState :: s -> (a, s) }

instance Functor (State s) where
  fmap f (State g) = State $ \s -> let (a, s') = g s in (f a, s')

instance Applicative (State s) where
  pure a = State $ \s -> (a, s)
  State f <*> State g = State $ \s ->
    let (h, s1) = f s
        (a, s2) = g s1
    in (h a, s2)

instance Monad (State s) where
  State g >>= k = State $ \s ->
    let (a, s1) = g s
    in runState (k a) s1

get :: State s s
get = State $ \s -> (s, s)

put :: s -> State s ()
put s = State $ const ((), s)

modify' :: (s -> s) -> State s ()
modify' f = get >>= put . f

counter :: State Int [Int]
counter = do
  a <- tick
  b <- tick
  mapM_ (const tick) [1 .. 3 :: Int]
  c <- tick
  return [a, b, c]
  where tick = do { n <- get; put (n + 1); return n }

main :: IO ()
main = do
  print (runState counter 10)
  print (runState (modify' (* 2) >> get) (21 :: Int))
  print (fst (runState (traverse (\x -> State (\s -> (x + s, s + 1))) [10, 20, 30 :: Int]) 0))
  print (runState (sequence_ [put 1, put (2 :: Int)]) 0)
